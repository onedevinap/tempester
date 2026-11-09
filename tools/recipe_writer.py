#!/usr/bin/env python3
"""Classify patch words by provenance and write them as a recipe (tools/recipe.py, Docs/patch-recipe-format.md).

Library, loaded by path like recipe.py. Input: the original DSI Main image and the words a patch writes, as
(address, new word, why) in patch order. Every word gets one class:
  (a)  ours    : new code or data we wrote, kept as a literal word;
  (b)  derived : a DSI word copied (relocated, or moved inside a rewritten routine), or a DSI instruction with one
                 field changed; written as an operation on the user's image ("copy", "copy" + "set");
  (c)  trivial : every j / jal (written as a jump computed from its target, whoever wrote it), our conditional
                 branches, and the trivial words of recipe.is_trivial (nop, erased flash, jr ra, constant loads,
                 register moves, stack-frame slots), kept literal;
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
lower one, so that no DSI-derived word stays literal."""
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
    """The original Main image as words, with the indexes the classifier needs."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.sha256 = hashlib.sha256(data).hexdigest()
        self.words = struct.unpack(f'<{len(data) // 4}I', data)
        self.where: dict[int, list[int]] = collections.defaultdict(list)
        for i, w in enumerate(self.words):
            self.where[w].append(i)
        self.pairs = collections.Counter(zip(self.words, self.words[1:]))

    def at(self, address: int) -> int:
        return self.words[(address - recipe.BASE) // 4]

    def holds(self, address: int) -> bool:
        return recipe.BASE <= address < recipe.BASE + len(self.data)

    def distinctive(self, first: int, second: int) -> bool:
        count = self.pairs[(first, second)]
        return count > 0 and not recipe.commonplace(first, second, count)


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


def rewritten_span(image: Image, words: list[Word]) -> list[int]:
    sites = sorted(w.address for w in words if image.at(w.address) != recipe.ERASED)
    clusters: list[list[int]] = []
    for a in sites:
        if clusters and a - clusters[-1][1] <= SPAN_GAP:
            clusters[-1][1] = a
        else:
            clusters.append([a, a])
    return [a for lo, hi in clusters for a in range(lo, hi + 4, 4)]


def literal(word: Word, cls: str, rule: str) -> Classified:
    return Classified(word, cls, rule, {'at': f'{word.address:#010x}', 'word': word.value.to_bytes(4, 'little').hex()})


def copy(word: Word, source: int, rule: str, fields: dict | None = None, cls: str = 'b') -> Classified:
    edit = {'at': f'{word.address:#010x}', 'copy': f'{source:#010x}'}
    if fields:
        edit['set'] = fields
    return Classified(word, cls, rule, edit)


def jump(word: Word) -> Classified:
    w, a = word.value, word.address
    target = (a + 4) & 0xF0000000 | (w & 0x03FFFFFF) << 2
    return Classified(word, 'c', 'jump', {'at': f'{a:#010x}', 'jal' if w >> 26 == 3 else 'j': f'{target:#010x}'})


def pair_source(image: Image, first: int, second: int) -> int | None:
    """An address x with original[x] = first and original[x + 4] = second, the pair being distinctive."""
    if not image.distinctive(first, second):
        return None
    for i in image.where.get(first, []):
        if i + 1 < len(image.words) and image.words[i + 1] == second:
            return recipe.BASE + 4 * i
    return None


class Classifier:
    """Applies the rules of the module docstring to the words of one patch, in patch order."""

    def __init__(self, image: Image, words: list[Word]) -> None:
        self.image = image
        self.span = rewritten_span(image, words)
        self.span_by_value: dict[int, list[int]] = collections.defaultdict(list)
        for a in self.span:
            self.span_by_value[image.at(a)].append(a)
        self.by_address = {w.address: w for w in words}
        self.copied_from: dict[int, int] = {}

    def nearest(self, candidates: list[int], target: int) -> int | None:
        return min(candidates, key=lambda x: (abs(x - target), x)) if candidates else None

    def classify(self, word: Word) -> Classified:
        image, w, a = self.image, word.value, word.address
        if is_jump(w):
            return jump(word)
        source = self.copied_from.get(a - 4)
        if source is not None and image.holds(source + 4) and image.at(source + 4) == w and w != recipe.ERASED:
            return self.copy(word, source + 4, 'copy, run continued')
        if w in (recipe.NOP, recipe.ERASED, recipe.JR_RA):
            return literal(word, 'c', 'trivial')
        found = one_field(image.at(a), w)
        if found is not None:
            return copy(word, a, 'one field changed in place', {found[0]: found[1]})
        if recipe.is_trivial(w):
            return literal(word, 'c', 'trivial')
        source = self.nearest([x for x in self.span_by_value.get(w, []) if x != a], a)
        if source is not None:
            return self.copy(word, source, 'copy from the rewritten span')
        nxt = self.by_address.get(a + 4)
        source = pair_source(image, w, nxt.value) if nxt else None
        if source is not None:
            return self.copy(word, source, 'copy of a distinctive DSI pair')
        for source in sorted(self.span, key=lambda x: (abs(x - a), x)):
            found = one_field(image.at(source), w)
            if found is not None:
                return copy(word, source, 'one field changed, from the span', {found[0]: found[1]})
        if is_branch(w):
            return literal(word, 'c', 'branch')
        return literal(word, 'a', 'ours')

    def copy(self, word: Word, source: int, rule: str) -> Classified:
        self.copied_from[word.address] = source
        return copy(word, source, rule)


def classify(image: Image, words: list[Word]) -> list[Classified]:
    classifier = Classifier(image, words)
    return guard_literals(image, [classifier.classify(w) for w in words])


def guard_literals(image: Image, items: list[Classified]) -> list[Classified]:
    """Rule (a=): two literal words next to each other in the recipe must not equal a distinctive DSI pair."""
    out = list(items)
    literals = [i for i, c in enumerate(out) if 'word' in c.edit]
    for i, j in zip(literals, literals[1:]):
        first, second = out[i].word, out[j].word
        source = pair_source(image, first.value, second.value)
        if source is None:
            continue
        if second.address != first.address + 4:
            # Next to each other in the recipe only (different places in memory): breaking the run is enough.
            out[j] = copy(second, source + 4, 'ours, next to an equal DSI pair in the recipe', cls='a=')
            continue
        out[i] = copy(first, source, 'ours, equal to a DSI pair', cls='a=')
        out[j] = copy(second, source + 4, 'ours, equal to a DSI pair', cls='a=')
    return out


def spells_instruction(text: str) -> bool:
    head = text.split(' ; ', 1)[0]
    mnemonic, _, operands = head.partition(' ')
    return mnemonic in MNEMONICS and (not operands or re.fullmatch(r'[\w$()+-]+(?:,[\w$()+-]+)*', operands) is not None)


def why_text(item: Classified, strip: bool) -> str:
    """Our comment only: an instruction copied from DSI is never spelled out in the recipe. strip is False for words
    read back from a recipe, whose comments were cleaned when it was written."""
    why = item.word.why
    if strip and item.cls in ('b', 'a=') and 'set' not in item.edit and spells_instruction(why):
        return why.split(' ; ', 1)[1] if ' ; ' in why else ''
    return why


def runs_on(last: dict, edit: dict) -> bool:
    count = last.get('count', 1)
    if int(edit['at'], 16) != int(last['at'], 16) + 4 * count:
        return False
    if 'word' in last and 'word' in edit:
        return last['word'] == edit['word']
    if 'copy' in last and 'copy' in edit and 'set' not in last and 'set' not in edit:
        return int(edit['copy'], 16) == int(last['copy'], 16) + 4 * count
    return False


def merge(items: list[Classified], strip: bool) -> list[dict]:
    """One edit per word, then runs folded: consecutive plain copies, and repeats of the same literal word."""
    edits: list[dict] = []
    whys: list[list[str]] = []
    for item in items:
        edit, why = dict(item.edit), why_text(item, strip)
        if edits and runs_on(edits[-1], edit):
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
    items = classify(image, words)
    out = {'format': recipe.FORMAT, 'name': head['name'], 'image': head.get('image', 'Main 1.5.0.2'),
           'input_sha256': image.sha256, 'explanation': head['explanation']}
    out.update({k: v for k, v in head.items() if k not in out})
    out['edits'] = merge(items, strip)
    resolved = dict(recipe.resolve(out, image.data, out['name']))
    if resolved != {w.address: w.value for w in words}:
        raise recipe.RecipeError(f'{head["name"]}: the recipe does not give back the patch words')
    return out, items
