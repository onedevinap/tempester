/* Boot: build the pages, the navigation and the connection pill, then ask Rust for the state. */
"use strict";
(function () {
  const { el, toast, errorText, pages, session, updateSession } =
    window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;

  const S = {
    disconnected: "Offline",
    /* The port itself is shown in the connection controls next to the pill (conn-bar.js). */
    connected: () => "Connected",
    error: "Connection error",
    fakeSuffix: " (fake)",
  };

  /* Tab order: by how often a page is used. The connection is always reachable in the top bar, so no page is
     needed to connect. Panel (the remote surface) first, then the Library, Play, then the Machine settings and
     monitor, which are looked at once in a while; Backup and OS update are rare, delicate jobs: they sit apart, at
     the right end, after a rule (still one click each; a "Tools" menu would cost a second click for no gain). A
     page missing from the list goes last, in registration order. */
  const TAB_ORDER = ["panel", "library", "play", "machine", "backup", "os"];
  const TOOL_PAGES = ["backup", "os"];

  function orderedPages(list) {
    const rank = (page) => {
      const at = TAB_ORDER.indexOf(page.id);
      return at < 0 ? TAB_ORDER.length : at;
    };
    return list.slice().sort((a, b) => rank(a) - rank(b));
  }

  const panels = {};
  const tabs = {};

  function show(id) {
    for (const page of pages) {
      panels[page.id].classList.toggle("shown", page.id === id);
      if (page.id === id) tabs[id].setAttribute("aria-current", "page");
      else tabs[page.id].removeAttribute("aria-current");
    }
    const page = pages.find((p) => p.id === id);
    if (page.onShow) page.onShow();
    history.replaceState(null, "", "#" + id);
  }

  function tabClass(id, toolsStarted) {
    if (!TOOL_PAGES.includes(id)) return false;
    return "tool" + (toolsStarted ? "" : " tools-start");
  }

  function buildShell() {
    const nav = document.getElementById("nav");
    const main = document.getElementById("pages");
    for (const page of pages) {
      panels[page.id] = el("section", {
        class: "page" + (page.fit ? " fit" : ""),
        id: "page-" + page.id,
        "aria-label": page.title,
      });
      main.append(panels[page.id]);
      page.build(panels[page.id]);
    }
    let toolsStarted = false;
    for (const page of orderedPages(pages)) {
      tabs[page.id] = el(
        "button",
        {
          type: "button",
          class: tabClass(page.id, toolsStarted),
          onclick: () => show(page.id),
        },
        page.title,
      );
      toolsStarted = toolsStarted || TOOL_PAGES.includes(page.id);
      nav.append(tabs[page.id]);
    }
  }

  function showConnection(m) {
    const pill = document.getElementById("conn");
    pill.className = "pill " + m.state;
    const text =
      m.state === "connected"
        ? S.connected(m.port)
        : m.state === "error"
          ? S.error
          : S.disconnected;
    pill.textContent = text + (m.fake ? S.fakeSuffix : "");
    pill.title = m.message || m.port || "";
    document.getElementById("fake-banner").hidden = !m.fake;
    document.body.classList.toggle("fake", !!m.fake);
    updateSession({
      connected: m.state === "connected",
      port: m.port || session.port,
      fake: !!m.fake,
    });
    if (m.state === "error") toast(m.message);
  }

  function start() {
    buildShell();
    bridge.on("conn", showConnection);
    bridge.on("page_error", (m) => toast("Page error: " + m.text));
    bridge.on("state", (m) =>
      updateSession({ channel: m.channel, port: session.port || m.port }),
    );
    bridge.on("channel", (m) => updateSession({ channel: m.channel }));
    bridge.on("error", (m) => {
      if (!m.during || m.during === "command") toast(errorText(m));
    });
    bridge.send("state");
    kit.setPageSwitcher(show);
    /* The app opens on the first tab (Panel), unless the address names a page. */
    const wanted = location.hash.slice(1);
    show(
      pages.some((p) => p.id === wanted)
        ? wanted
        : orderedPages(pages)[0].id,
    );
  }

  start();
})();
