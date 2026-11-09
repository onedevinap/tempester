/* SHA-256 evidence row of the OS pages: a mark (check or warning), one line of text, the hashes shortened in
   monospace, a copy button and an info badge with the full values. The row has a fixed height and every part keeps
   its slot, so showing or clearing a result never moves anything. Used by the OS update page and the installed-OS
   check (os-verify.js). */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.osHash = (function () {
  const { el, toast } = window.Tempester.kit;
  const { icon, button, info } = window.Tempester.icons;

  const S = {
    copy: "Copy the full SHA-256 values",
    full: "Full SHA-256 values",
    copied: "SHA-256 values copied.",
    copyFailed:
      "The text could not be copied. Open the full values and select them.",
  };

  /* 12 hexadecimal characters are enough to compare by eye; the full value is one click away. */
  function short(hash) {
    return hash ? hash.slice(0, 12) + "…" : "";
  }

  /* The clipboard API may be refused in the web view: a hidden text area and the old command are the fallback. */
  async function writeClipboard(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (e) {
      const area = el("textarea", {
        class: "hidden-input",
        "aria-hidden": "true",
      });
      area.value = text;
      document.body.append(area);
      area.select();
      const done = document.execCommand("copy");
      area.remove();
      return done;
    }
  }

  /* `show({ tone, text, hashes: [{ label, value, shownAs, fullOnly }] })`; `tone` is "good", "warn", "bad" or "".
     The row shows `shownAs || label` and the shortened value of each hash that is not `fullOnly`; the info badge
     and the copy button always have every value in full. */
  function evidence() {
    const mark = el("span", { class: "evidence-mark" });
    const text = el("span", { class: "statusline", role: "status" });
    const shorts = el("span", { class: "evidence-hashes mono" });
    const full = el("div", { class: "evidence-full" });
    let copyValue = "";
    const copyButton = button("copy", S.copy, async () => {
      toast((await writeClipboard(copyValue)) ? S.copied : S.copyFailed);
    });
    const badge = info(full, S.full, "right");
    const node = el(
      "div",
      { class: "evidence" },
      mark,
      text,
      shorts,
      copyButton,
      badge,
    );

    function show(result) {
      const hashes = result.hashes || [];
      const tone = result.tone || "";
      mark.replaceChildren(
        ...(tone === "good"
          ? [icon("check")]
          : tone === "warn" || tone === "bad"
            ? [icon("warning")]
            : []),
      );
      mark.setAttribute("class", "evidence-mark " + tone);
      text.textContent = result.text || "";
      text.setAttribute("class", "statusline " + tone);
      text.setAttribute("title", result.text || "");
      shorts.replaceChildren(
        ...hashes
          .filter((h) => !h.fullOnly)
          .map((h) =>
          el("span", {}, (h.shownAs || h.label) + " " + short(h.value) + "  "),
        ),
      );
      full.replaceChildren(
        ...hashes.map((h) =>
          el(
            "p",
            {},
            h.label + " SHA-256:",
            el("br"),
            el("span", { class: "mono hash-full" }, h.value),
          ),
        ),
      );
      copyValue = hashes
        .map((h) => h.label + " SHA-256 " + h.value)
        .join("\n");
      copyButton.hidden = !hashes.length;
      badge.hidden = !hashes.length;
    }

    show({});
    return { node, show, copyText: () => copyValue };
  }

  return { short, evidence, writeClipboard };
})();
