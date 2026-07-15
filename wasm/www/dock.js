export function initDock() {
  const charPanel = document.querySelector(".control-panel");
  const dockBtn = document.getElementById("dockbtn");
  const dockTab = document.getElementById("docktab");
  const sentinel = document.createElement("div");
  const spacer = document.createElement("div");
  charPanel.before(sentinel, spacer);
  let charFloating = false;
  let charDocked = localStorage.getItem("shipdb.dock") === "1";
  let inlineHeight = 0;

  const updateDock = () => {
    charPanel.classList.toggle("floating", charFloating);
    charPanel.classList.toggle("docked", charFloating && charDocked);
    dockTab.hidden = !(charFloating && charDocked);
    if (!charFloating) inlineHeight = charPanel.offsetHeight;
    spacer.style.height = charFloating ? inlineHeight + "px" : "";
  };

  dockBtn.addEventListener("click", () => {
    charDocked = true;
    try {
      localStorage.setItem("shipdb.dock", "1");
    } catch (e) {}
    updateDock();
  });
  dockTab.addEventListener("click", () => {
    charDocked = false;
    try {
      localStorage.setItem("shipdb.dock", "0");
    } catch (e) {}
    updateDock();
  });

  new IntersectionObserver(([e]) => {
    charFloating = e.boundingClientRect.top < 0;
    updateDock();
  }).observe(sentinel);

  updateDock();
}
