if (!globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__) {
  globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__ = true;

  let generation = "";
  let refs = new Map();

  const candidateSelector = [
    "a[href]", "button", "input", "textarea", "select", "summary", "details", "[contenteditable='true']",
    "[role='button']", "[role='link']", "[role='checkbox']", "[role='radio']", "[role='switch']", "[role='tab']",
    "[role='menuitem']", "[role='option']", "[tabindex]:not([tabindex='-1'])", "[onclick]",
  ].join(",");

  function clean(value, max = 300) {
    return String(value ?? "").replace(/[\u0000-\u001f\u007f]+/g, " ").replace(/\s+/g, " ").trim().slice(0, max);
  }

  function visible(element) {
    if (!(element instanceof Element)) return false;
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity) !== 0 && rect.width > 1 && rect.height > 1;
  }

  function labelledBy(element) {
    const ids = clean(element.getAttribute("aria-labelledby"), 500).split(" ").filter(Boolean);
    const root = element.getRootNode();
    return ids.map((id) => clean(root.getElementById?.(id)?.textContent || document.getElementById(id)?.textContent)).filter(Boolean).join(" ");
  }

  function accessibleName(element) {
    const direct = clean(element.getAttribute("aria-label"));
    if (direct) return direct;
    const referenced = labelledBy(element);
    if (referenced) return referenced;
    if (element.labels?.length) {
      const label = clean([...element.labels].map((item) => item.innerText || item.textContent).join(" "));
      if (label) return label;
    }
    return clean(
      element.alt || element.title || element.placeholder ||
      ((element instanceof HTMLInputElement && !["password", "hidden"].includes(element.type)) ? element.value : "") ||
      element.innerText || element.textContent || element.getAttribute("name") || element.id,
    );
  }

  function roleOf(element) {
    const explicit = clean(element.getAttribute("role"), 60);
    if (explicit) return explicit;
    const tag = element.tagName.toLowerCase();
    if (tag === "a") return "link";
    if (tag === "button" || tag === "summary") return "button";
    if (tag === "select") return "select";
    if (tag === "textarea") return "textbox";
    if (tag === "input") {
      if (["checkbox", "radio", "button", "submit", "reset"].includes(element.type)) return element.type;
      return "textbox";
    }
    return tag;
  }

  function describe(element, ref) {
    const rect = element.getBoundingClientRect();
    const type = clean(element.getAttribute("type"), 60).toLowerCase();
    const autocomplete = clean(element.getAttribute("autocomplete"), 100).toLowerCase();
    const fieldName = clean(element.getAttribute("name"), 100);
    const sensitive = type === "password" || /password|one-time-code|cc-number|cc-csc/i.test(`${autocomplete} ${fieldName}`);
    return {
      ref,
      role: roleOf(element),
      name: accessibleName(element),
      type,
      autocomplete,
      fieldName,
      disabled: Boolean(element.disabled) || element.getAttribute("aria-disabled") === "true",
      checked: "checked" in element ? Boolean(element.checked) : undefined,
      selected: "selected" in element ? Boolean(element.selected) : undefined,
      sensitive,
      inViewport: rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth,
      bounds: {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      },
      tree: element.getRootNode() instanceof ShadowRoot ? "shadow" : "document",
    };
  }

  function composedCandidates(root = document) {
    const found = [];
    const seen = new Set();
    const visit = (scope) => {
      for (const element of scope.querySelectorAll(candidateSelector)) {
        if (!seen.has(element)) {
          seen.add(element);
          found.push(element);
        }
      }
      for (const element of scope.querySelectorAll("*")) {
        if (element.shadowRoot) visit(element.shadowRoot);
      }
    };
    visit(root);
    return found;
  }

  function snapshot() {
    generation = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
    refs = new Map();
    const elements = [];
    for (const element of composedCandidates()) {
      if (elements.length >= 500 || !visible(element)) continue;
      const ref = `e${elements.length + 1}`;
      refs.set(ref, element);
      elements.push(describe(element, ref));
    }
    return {
      generation,
      title: clean(document.title, 500),
      url: `${location.origin}${location.pathname}`,
      viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
      scroll: { x: Math.round(scrollX), y: Math.round(scrollY), maxY: Math.max(0, document.documentElement.scrollHeight - innerHeight) },
      selectedText: clean(window.getSelection()?.toString(), 5_000),
      bodyText: clean(document.body?.innerText, 20_000),
      elements,
    };
  }

  function resolve(ref, requestedGeneration) {
    if (!generation || requestedGeneration !== generation) throw new Error("STALE_SNAPSHOT: observe the page again before acting");
    const element = refs.get(ref);
    if (!element?.isConnected) throw new Error("STALE_REF: the element changed; observe the page again");
    return element;
  }

  function setNativeValue(element, value) {
    if (element instanceof HTMLInputElement) {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
      descriptor?.set?.call(element, value);
    } else if (element instanceof HTMLTextAreaElement) {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
      descriptor?.set?.call(element, value);
    } else if (element.isContentEditable) {
      element.textContent = value;
    } else {
      throw new Error("Element is not fillable");
    }
    element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
  }

  async function handle(message) {
    switch (message.method) {
      case "snapshot":
        return snapshot();
      case "assertGeneration":
        if (!generation || message.generation !== generation) throw new Error("STALE_SNAPSHOT: observe the page again before acting");
        return { current: true };
      case "describe": {
        const element = resolve(message.ref, message.generation);
        return describe(element, message.ref);
      }
      case "prepareClick": {
        const element = resolve(message.ref, message.generation);
        element.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
        await new Promise((resolveFrame) => requestAnimationFrame(() => resolveFrame()));
        return describe(element, message.ref);
      }
      case "clickFallback": {
        const element = resolve(message.ref, message.generation);
        element.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
        element.focus({ preventScroll: true });
        element.click();
        return { clicked: true, trusted: false };
      }
      case "fill": {
        const element = resolve(message.ref, message.generation);
        const description = describe(element, message.ref);
        if (description.sensitive && !message.allowSensitive) throw new Error("SENSITIVE_FIELD: enter this value manually");
        element.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
        element.focus({ preventScroll: true });
        setNativeValue(element, String(message.text ?? ""));
        return { filled: true };
      }
      case "select": {
        const element = resolve(message.ref, message.generation);
        if (!(element instanceof HTMLSelectElement)) throw new Error("Element is not a select control");
        const option = [...element.options].find((item) => item.value === message.value || item.label === message.value);
        if (!option) throw new Error("No option matched that value or label");
        element.value = option.value;
        element.dispatchEvent(new Event("input", { bubbles: true }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
        return { selected: option.value, label: option.label };
      }
      case "scroll":
        window.scrollBy({ left: Number(message.deltaX) || 0, top: Number(message.deltaY) || 0, behavior: "instant" });
        return { x: Math.round(scrollX), y: Math.round(scrollY) };
      default:
        throw new Error("Unknown content command");
    }
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (message?.type !== "LBB_CONTENT") return undefined;
    handle(message)
      .then((result) => sendResponse({ ok: true, result }))
      .catch((error) => sendResponse({ ok: false, error: error.message }));
    return true;
  });
}
