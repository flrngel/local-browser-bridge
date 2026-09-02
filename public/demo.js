const log = document.getElementById("event-log");
const record = (value) => {
  log.textContent = value;
  document.body.dataset.lastAction = value;
};

document.getElementById("route-state").textContent = `Route: ${location.pathname}${location.search}`;

document.getElementById("demo-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const data = new FormData(event.currentTarget);
  document.getElementById("result").textContent = `Hello, ${data.get("name") || "visitor"}. ${data.get("color")} selected.`;
  record(`submit:${data.get("name")}:${data.get("color")}`);
});

document.getElementById("coordinate-button").addEventListener("click", (event) => {
  record(`coordinate:${event.isTrusted}`);
});
document.getElementById("bottom-button").addEventListener("click", () => record("bottom-click"));
document.getElementById("keyboard-target").addEventListener("input", (event) => record(`input:${event.currentTarget.value}`));
document.getElementById("keyboard-target").addEventListener("keydown", (event) => record(`key:${event.key}`));

const shadowRoot = document.getElementById("shadow-host").attachShadow({ mode: "open" });
shadowRoot.innerHTML = '<button id="shadow-action" type="button" aria-label="Shadow action">Shadow action</button>';
shadowRoot.getElementById("shadow-action").addEventListener("click", () => record("shadow-click"));
