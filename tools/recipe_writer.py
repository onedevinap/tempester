#!/usr/bin/env python3
"""Classify patch words by provenance and write them as a recipe (tools/recipe.py, Docs/patch-recipe-format.md).

Library, loaded by path like recipe.py. Input: the original DSI image (Main, or Voice with cpu='dspic33f') and the
words a patch writes, as (address, new word, why) in patch order. Every word gets one class:
  (a)  ours    : new code or data we wrote, kept as a literal word;
  (b)  derived : a DSI word copied (relocated, or moved inside a rewritten routine), or a DSI instruction with one
                 field changed; written as an operation on the user's image ("copy", "copy" + "set");
  (c)  trivial : every j / jal (Voice: goto, call, bra, rcall), written as a jump computed from its target, whoever
                 wrote it; our conditional branches, and the trivial words of recipe.is_trivial (nop, erased flash,
                 jr ra, constant loads, register moves, stack-frame slots; Voice: recipe.pic24_is_trivial), literal;
  (a=) ours, but two literal words next to each other in the recipe would equal a distinctive pair of DSI words
       (recipe.commonplace is false): written as a copy of that DSI location instead, so that no literal run of a
       recipe equals DSI's code.
Rules, in order (first match wins), for a word w written at address a:
  1. w is a j / jal                                                           -> (c) jump
  2. the previous word of the patch is a copy from x, and w is the original word at x + 4 (not erased flash)
                                                                              -> (b) copy (run continued)
  3. w is nop, erased flash or jr ra                                          -> (c) literal
  4. w is the original word at a with exactly one instruction field changed (opcode and function kept)
                                                                              -> (b) copy of a + set
  5. w is another trivial word (constant load, register move, stack-frame slot) -> (c) literal
  6. w equals an original word of the patch's rewritten span (see below)      -> (b) copy, nearest source
  7. w and the next patch word equal a distinctive pair of consecutive original words anywhere -> (b) copy (run)
  8. w is a word of the rewritten span with exactly one field changed         -> (b) copy + set, nearest source
  9. w is a conditional branch                                                -> (c) literal
 10. otherwise                                                                -> (a) literal
The rewritten span of a patch: the original DSI words it overwrites (not erased flash), each cluster of them
widened to its first and last address (words left unchanged inside a rewritten routine belong to it).
Rules 4 and 8 count as derived any word one field away from a DSI word it replaces: an upper bound on (b), never a
lower one, so that no DSI-derived word stays literal.
Voice: the same rules on 24-bit words, one PC step (2) per word: x + 4 above reads "the next word", rule 3 is nop,
erased flash and return, the fields are recipe.PIC24_FIELDS (opcode bits 23-20 kept), and a goto / call is one edit
for its two words (its second word written only when it changes)."""
import collections
import hashlib
import importlib.util
import re
import struct
from dataclasses import dataclass
from pathlib import Path

_spec = importlib.util.spec_from_file_location('recipe', Path(__file__).resolve().parent / 'recipe.py')
recipe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(recipe)

SPAN_GAP = 64  # bytes: two overwritten DSI words closer than this belong to the same rewritten routine
R_TYPES = (0x00, 0x1C, 0x1F)  # SPECIAL, SPECIAL2, SPECIAL3: function field in bits 0-5
# Mnemonics the assembler listing writes at the head of a comment ("lw v0,0x18(sp) ; comment").
MNEMONICS = frozenset(
    'add addi addiu addu and andi b beq beql bgez bgtz blez bltz bne bnel clear div divu j jal jalr jr lb lbu lh lhu '
    'li lui lw mfhi mflo move movn movz mul mult multu negu nop nor not or ori sb seb seh sh sll sllv slt slti '
    'sltiu sltu sra srav srl srlv sub subu sw teq xor xori'.split())
# The same for the dsPIC33F listing ("mov.w #0x1234,W6 ; comment"), size suffix (.w, .b, .uu…) removed.
PIC24_MNEMONICS = frozenset(
    'add addc and asr bclr bra bset btg btsc btss btst btsts call clr com cp cp0 cpb cpseq cpsgt cpslt cpsne dec '
    'dec2 disi div exch goto inc inc2 ior lnk lsr mov mul neg nop nopr pop push rcall repeat reset retfie retlw '
    'return rlc rlnc rrc rrnc se setm sl sub subb subbr subr swap tblrdh tblrdl tblwth tblwtl ulnk xor ze'.split())


def is_jump(w: int) -> bool:
    return w >> 26 in (2, 3)


def is_branch(w: int) -> bool:
    op, rt = w >> 26, (w >> 16) & 0x1F
    return op in (4, 5, 6, 7, 0x14, 0x15, 0x16, 0x17) or (op == 1 and rt in (0, 1, 2, 3, 0x10, 0x11, 0x12, 0x13))


def one_field(original: int, new: int) -> tuple[str, int] | None:
    """The single instruction field in which new differs from original, opcode (and function) kept."""
    if original >> 26 != new >> 26 or original in (recipe.ERASED, recipe.NOP) or is_jump(original):
        return None
    r_type = original >> 26 in R_TYPES
    if r_type and (original ^ new) & 0x3F:
        return None
    diff = original ^ new
    for name in ('rs', 'rt', 'rd', 'sa') if r_type else ('rs', 'rt', 'imm'):
        lo, width = recipe.FIELDS[name]
        mask = ((1 << width) - 1) << lo
        if diff & ~mask == 0:
            return name, (new & mask) >> lo
    return None


def jump(word: 'Word') -> 'Classified':
    w, a = word.value, word.address
    target = (a + 4) & 0xF0000000 | (w & 0x03FFFFFF) << 2
    return Classified(word, 'c', 'jump', {'at': f'{a:#010x}', 'jal' if w >> 26 == 3 else 'j': f'{target:#010x}'})


# ---- Voice (dsPIC33F) ------------------------------------------------------------------------------------------------

PIC24_JUMP_OPS = {op: kind for kind, op in recipe.PIC24_JUMPS.items()}


def pic24_is_jump(w: int) -> bool:
    return w >> 16 in PIC24_JUMP_OPS


def pic24_is_branch(w: int) -> bool:
    return w >> 20 == 0x3 and w >> 16 != 0x37


def pic24_one_field(original: int, new: int) -> tuple[str, int] | None:
    """The single field of recipe.PIC24_FIELDS (narrowest first) in which new differs, opcode bits 23-20 kept."""
    if (original ^ new) & 0xF00000 or original in (recipe.PIC24_ERASED, recipe.PIC24_NOP) or pic24_is_jump(original):
        return None
    diff = original ^ new
    for name, (lo, width) in sorted(recipe.PIC24_FIELDS.items(), key=lambda f: f[1][1]):
        mask = ((1 << width) - 1) << lo
        if diff & ~mask == 0:
            return name, (new & mask) >> lo
    return None


def pic24_jump(word: 'Word', following: 'Word | None') -> 'Classified':
    """goto / call (target from this word and the next one) or bra / rcall (from the offset), as one edit."""
    w, a = word.value, word.address
    kind = PIC24_JUMP_OPS[w >> 16]
    if kind in ('goto', 'call'):
        high = following.value if following is not None else 0
        target = (high & 0x7F) << 16 | w & 0xFFFE
    else:
        offset = w & 0xFFFF
        target = a + 2 + 2 * (offset - 0x10000 if offset & 0x8000 else offset)
    return Classified(word, 'c', 'jump', {'at': f'0x{a:06x}', kind: f'0x{target:06x}'})


@dataclass(frozen=True, slots=True)
class Isa:
    """What the classifier needs of a processor; MIPS = the Main (its constants as before), PIC24 = the Voice."""
    cpu: str
    step: int            # address units per word
    word_bytes: int
    base: int
    erased: int
    plain: frozenset     # rule 3
    mnemonics: frozenset
    address_digits: int
    image_name: str

    def address_text(self, a: int) -> str:
        return f'{a:#010x}' if self.address_digits == 8 else f'0x{a:0{self.address_digits}x}'

    def jumpy(self, w: int) -> bool:
        return is_jump(w) if self.cpu == 'pic32mx' else pic24_is_jump(w)

    def branch(self, w: int) -> bool:
        return is_branch(w) if self.cpu == 'pic32mx' else pic24_is_branch(w)

    def field(self, original: int, new: int) -> tuple[str, int] | None:
        return one_field(original, new) if self.cpu == 'pic32mx' else pic24_one_field(original, new)

    def trivial(self, w: int) -> bool:
        return recipe.is_trivial(w) if self.cpu == 'pic32mx' else recipe.pic24_is_trivial(w)

    def commonplace(self, first: int, second: int, count: int) -> bool:
        fn = recipe.commonplace if self.cpu == 'pic32mx' else recipe.pic24_commonplace
        return fn(first, second, count)


MIPS = Isa('pic32mx', 4, 4, recipe.BASE, recipe.ERASED, frozenset((recipe.NOP, recipe.ERASED, recipe.JR_RA)),
           MNEMONICS, 8, 'Main 1.5.0.2')
PIC24 = Isa('dspic33f', 2, 3, 0, recipe.PIC24_ERASED,
            frozenset((recipe.PIC24_NOP, recipe.PIC24_ERASED, recipe.PIC24_RETURN)), PIC24_MNEMONICS, 6, 'Voice 1.5')
ISAS = {isa.cpu: isa for isa in (MIPS, PIC24)}


@dataclass(frozen=True, slots=True)
class Word:
    address: int
    value: int
    why: str


@dataclass(frozen=True, slots=True)
class Classified:
    word: Word
    cls: str      # 'a', 'a=', 'b', 'c'
    rule: str     # short label of the rule that decided
    edit: dict    # the single-word edit, before runs are folded


class Image:
    """The original image (Main, or Voice with cpu='dspic33f') as words, with the indexes the classifier needs."""

    def __init__(self, data: bytes, cpu: str = 'pic32mx') -> None:
        self.data = data
        self.isa = ISAS[cpu]
        self.sha256 = hashlib.sha256(data).hexdigest()
        if cpu == 'pic32mx':
            self.words = struct.unpack(f'<{len(data) // 4}I', data)
        else:
            self.words = tuple(int.from_bytes(data[i:i + 3], 'little') for i in range(0, len(data) - 2, 3))
        self.where: dict[int, list[int]] = collections.defaultdict(list)
        for i, w in enumerate(self.words):
            self.where[w].append(i)
        self.pairs = collections.Counter(zip(self.words, self.words[1:]))

    def at(self, address: int) -> int:
        return self.words[(address - self.isa.base) // self.isa.step]

    def holds(self, address: int) -> bool:
        return self.isa.base <= address < self.isa.base + len(self.words) * self.isa.step

    def distinctive(self, first: int, second: int) -> bool:
        count = self.pairs[(first, second)]
        return count > 0 and not self.isa.commonplace(first, second, count)


def rewritten_span(image: Image, words: list[Word]) -> list[int]:
    isa = image.isa
    sites = sorted(w.address for w in words if image.at(w.address) != isa.erased)
    clusters: list[list[int]] = []
    gap = SPAN_GAP * isa.step // 4
    for a in sites:
        if clusters and a - clusters[-1][1] <= gap:
            clusters[-1][1] = a
        else:
            clusters.append([a, a])
    return [a for lo, hi in clusters for a in range(lo, hi + isa.step, isa.step)]


def literal(word: Word, cls: str, rule: str, isa: Isa = MIPS) -> Classified:
    return Classified(word, cls, rule, {'at': isa.address_text(word.address),
                                        'word': word.value.to_bytes(isa.word_bytes, 'little').hex()})


def copy(word: Word, source: int, rule: str, fields: dict | None = None, cls: str = 'b', isa: Isa = MIPS) -> Classified:
    edit = {'at': isa.address_text(word.address), 'copy': isa.address_text(source)}
    if fields:
        edit['set'] = fields
    return Classified(word, cls, rule, edit)


def pair_source(image: Image, first: int, second: int) -> int | None:
    """An address x with original[x] = first and original[x + 4] = second, the pair being distinctive."""
    if not image.distinctive(first, second):
        return None
    for i in image.where.get(first, []):
        if i + 1 < len(image.words) and image.words[i + 1] == second:
            return image.isa.base + image.isa.step * i
    return None


class Classifier:
    """Applies the rules of the module docstring to the words of one patch, in patch order."""

    def __init__(self, image: Image, words: list[Word]) -> None:
        self.image = image
        self.isa = image.isa
        self.span = rewritten_span(image, words)
        self.span_by_value: dict[int, list[int]] = collections.defaultdict(list)
        for a in self.span:
            self.span_by_value[image.at(a)].append(a)
        self.by_address = {w.address: w for w in words}
        self.copied_from: dict[int, int] = {}

    def nearest(self, candidates: list[int], target: int) -> int | None:
        return min(candidates, key=lambda x: (abs(x - target), x)) if candidates else None

    def classify(self, word: Word) -> Classified:
        image, isa, w, a = self.image, self.isa, word.value, word.address
        if isa.jumpy(w):
            if isa is MIPS:
                return jump(word)
            second = self.by_address.get(a + 2) or (Word(a + 2, image.at(a + 2), '') if image.holds(a + 2) else None)
            return pic24_jump(word, second)
        source = self.copied_from.get(a - isa.step)
        if source is not None and image.holds(source + isa.step) and image.at(source + isa.step) == w \
                and w != isa.erased:
            return self.copy(word, source + isa.step, 'copy, run continued')
        if w in isa.plain:
            return literal(word, 'c', 'trivial', isa)
        found = isa.field(image.at(a), w)
        if found is not None:
            return copy(word, a, 'one field changed in place', {found[0]: found[1]}, isa=isa)
        if isa.trivial(w):
            return literal(word, 'c', 'trivial', isa)
        source = self.nearest([x for x in self.span_by_value.get(w, []) if x != a], a)
        if source is not None:
            return self.copy(word, source, 'copy from the rewritten span')
        nxt = self.by_address.get(a + isa.step)
        source = pair_source(image, w, nxt.value) if nxt else None
        if source is not None:
            return self.copy(word, source, 'copy of a distinctive DSI pair')
        for source in sorted(self.span, key=lambda x: (abs(x - a), x)):
            found = isa.field(image.at(source), w)
            if found is not None:
                return copy(word, source, 'one field changed, from the span', {found[0]: found[1]}, isa=isa)
        if isa.branch(w):
            return literal(word, 'c', 'branch', isa)
        return literal(word, 'a', 'ours', isa)

    def copy(self, word: Word, source: int, rule: str) -> Classified:
        self.copied_from[word.address] = source
        return copy(word, source, rule, isa=self.isa)


def jump_second_words(image: Image, words: list[Word]) -> set[int]:
    """Voice: the addresses of the second word of each goto / call, written by the edit of its first word."""
    if image.isa is MIPS:
        return set()
    return {w.address + 2 for w in words if w.value >> 16 in (0x02, 0x04)}


def classify(image: Image, words: list[Word]) -> list[Classified]:
    classifier = Classifier(image, words)
    seconds = jump_second_words(image, words)
    return guard_literals(image, [classifier.classify(w) for w in words if w.address not in seconds])


def guard_literals(image: Image, items: list[Classified]) -> list[Classified]:
    """Rule (a=): two literal words next to each other in the recipe must not equal a distinctive DSI pair."""
    out = list(items)
    isa = image.isa
    literals = [i for i, c in enumerate(out) if 'word' in c.edit]
    for i, j in zip(literals, literals[1:]):
        first, second = out[i].word, out[j].word
        source = pair_source(image, first.value, second.value)
        if source is None:
            continue
        if second.address != first.address + isa.step:
            # Next to each other in the recipe only (different places in memory): breaking the run is enough.
            out[j] = copy(second, source + isa.step, 'ours, next to an equal DSI pair in the recipe', cls='a=',
                          isa=isa)
            continue
        out[i] = copy(first, source, 'ours, equal to a DSI pair', cls='a=', isa=isa)
        out[j] = copy(second, source + isa.step, 'ours, equal to a DSI pair', cls='a=', isa=isa)
    return out


def spells_instruction(text: str, mnemonics: frozenset = MNEMONICS) -> bool:
    head = text.split(' ; ', 1)[0]
    mnemonic, _, operands = head.partition(' ')
    if mnemonics is PIC24_MNEMONICS:
        mnemonic = mnemonic.split('.', 1)[0]
        return mnemonic in mnemonics and (not operands or
                                          re.fullmatch(r'[\w$()+#\[\]-]+(?:,[\w$()+#\[\]-]+)*', operands) is not None)
    return mnemonic in mnemonics and (not operands or re.fullmatch(r'[\w$()+-]+(?:,[\w$()+-]+)*', operands) is not None)


def why_text(item: Classified, strip: bool, mnemonics: frozenset = MNEMONICS) -> str:
    """Our comment only: an instruction copied from DSI is never spelled out in the recipe. strip is False for words
    read back from a recipe, whose comments were cleaned when it was written."""
    why = item.word.why
    if strip and item.cls in ('b', 'a=') and 'set' not in item.edit and spells_instruction(why, mnemonics):
        return why.split(' ; ', 1)[1] if ' ; ' in why else ''
    return why


def runs_on(last: dict, edit: dict, step: int = 4) -> bool:
    count = last.get('count', 1)
    if int(edit['at'], 16) != int(last['at'], 16) + step * count:
        return False
    if 'word' in last and 'word' in edit:
        return last['word'] == edit['word']
    if 'copy' in last and 'copy' in edit and 'set' not in last and 'set' not in edit:
        return int(edit['copy'], 16) == int(last['copy'], 16) + step * count
    return False


def merge(items: list[Classified], strip: bool, isa: Isa = MIPS) -> list[dict]:
    """One edit per word, then runs folded: consecutive plain copies, and repeats of the same literal word."""
    edits: list[dict] = []
    whys: list[list[str]] = []
    for item in items:
        edit, why = dict(item.edit), why_text(item, strip, isa.mnemonics)
        if edits and runs_on(edits[-1], edit, isa.step):
            edits[-1]['count'] = edits[-1].get('count', 1) + 1
            if why and why not in whys[-1]:
                whys[-1].append(why)
            continue
        edits.append(edit)
        whys.append([why] if why else [])
    for edit, texts in zip(edits, whys):
        if texts:
            edit['why'] = ' | '.join(texts)
    return edits


def encode(image: Image, words: list[Word], head: dict, strip: bool = True) -> tuple[dict, list[Classified]]:
    """The recipe of a patch: head (name, image, explanation, other keys kept) plus its edits, checked to give
    back exactly the patch words on the image. strip: remove the instruction text of copied words' comments."""
    isa = image.isa
    items = classify(image, words)
    out = {'format': recipe.FORMAT}
    if isa is not MIPS:
        out['cpu'] = isa.cpu
    out.update({'name': head['name'], 'image': head.get('image', isa.image_name), 'input_sha256': image.sha256,
                'explanation': head['explanation']})
    out.update({k: v for k, v in head.items() if k not in out})
    out['edits'] = merge(items, strip, isa)
    resolved = dict(recipe.resolve(out, image.data, out['name']))
    if resolved != {w.address: w.value for w in words}:
        raise recipe.RecipeError(f'{head["name"]}: the recipe does not give back the patch words')
    return out, items
