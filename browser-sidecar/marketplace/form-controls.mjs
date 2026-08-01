/**
 * DOM control abstractions for Facebook Marketplace vehicle create form.
 * Used inside page.evaluate — must be self-contained strings for injection.
 */
export const FORM_CONTROL_HELPERS = `
function q(selectors) {
  for (const s of selectors || []) {
    try {
      const el = document.querySelector(s);
      if (el) return el;
    } catch (_e) {}
  }
  return null;
}

function labelFor(text) {
  const escaped = text.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\\\$&");
  const rx = new RegExp("^\\\\s*" + escaped + "\\\\s*$", "i");
  const candidates = Array.from(
    document.querySelectorAll('label, span, div[role="button"], div[role="presentation"]'),
  );
  for (const l of candidates) {
    if (rx.test(l.textContent || "")) return l;
  }
  return null;
}

function inputNear(labelEl) {
  if (!labelEl) return null;
  const forId = labelEl.getAttribute && labelEl.getAttribute("for");
  if (forId) {
    const el = document.getElementById(forId);
    if (el) return el;
  }
  const scope = labelEl.closest("div,section,label,form") || labelEl.parentElement;
  if (!scope) return null;
  return scope.querySelector(
    'input:not([type=hidden]), textarea, [role="combobox"], [role="listbox"], [contenteditable="true"]',
  );
}

function findByAria(patterns) {
  for (const p of patterns || []) {
    const rx = new RegExp(p, "i");
    const els = Array.from(document.querySelectorAll("[aria-label], [placeholder]"));
    for (const el of els) {
      const label = el.getAttribute("aria-label") || el.getAttribute("placeholder") || "";
      if (rx.test(label)) return el;
    }
  }
  return null;
}

function resolveControl(field) {
  for (const label of field.labels || []) {
    const lab = labelFor(label);
    const el = inputNear(lab);
    if (el) return el;
  }
  const aria = findByAria(field.ariaPatterns);
  if (aria) return aria;
  return q(field.cssFallbacks);
}

function readControlValue(el) {
  if (!el) return "";
  if (el.tagName === "TEXTAREA" || el.tagName === "INPUT") {
    return (el.value || "").trim();
  }
  return (el.textContent || el.innerText || "").trim();
}

function setNativeValue(el, value) {
  const proto =
    el.tagName === "TEXTAREA"
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
  if (setter) {
    setter.call(el, value);
  } else {
    el.value = value;
  }
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

async function fillTextControl(el, value) {
  el.scrollIntoView({ block: "center" });
  el.focus();
  await new Promise((r) => setTimeout(r, 120));
  setNativeValue(el, String(value));
  await new Promise((r) => setTimeout(r, 120));
  el.blur();
  return readControlValue(el);
}

async function pickComboboxOption(el, optionText) {
  if (!el || !optionText) return false;
  el.scrollIntoView({ block: "center" });
  el.click();
  await new Promise((r) => setTimeout(r, 350));
  const escaped = String(optionText).replace(/[.*+?^${}()|[\\]\\\\]/g, "\\\\$&");
  const rx = new RegExp("^\\\\s*" + escaped + "\\\\s*", "i");
  const items = Array.from(
    document.querySelectorAll('[role="option"], [role="menuitem"], [role="menuitemradio"]'),
  );
  const hit = items.find((i) => rx.test(i.textContent || ""));
  if (!hit) {
    document.body.click();
    return false;
  }
  hit.click();
  await new Promise((r) => setTimeout(r, 250));
  return true;
}

function normalizeDigits(value) {
  return String(value || "").replace(/[^0-9.]/g, "");
}

function normalizePrice(value) {
  return normalizeDigits(value);
}

function normalizeMileage(value) {
  return normalizeDigits(value);
}
`;
