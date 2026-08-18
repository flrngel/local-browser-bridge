document.getElementById("demo-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const data = new FormData(event.currentTarget);
  document.getElementById("result").textContent = `Hello, ${data.get("name") || "visitor"}. ${data.get("color")} selected.`;
});
