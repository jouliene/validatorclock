// Element builder shared by both pages. Data reaches the page as text nodes,
// never as HTML: innerHTML lives in this file and only takes constant markup.
function el(tag, options = {}, children = []) {
  const node = document.createElement(tag);
  const settings = typeof options === "string" ? { className: options } : options || {};

  if (settings.className) {
    node.className = settings.className;
  }
  if (settings.text !== undefined && settings.text !== null) {
    node.textContent = String(settings.text);
  }
  for (const [name, value] of Object.entries(settings.attrs || {})) {
    if (value === undefined || value === null || value === false) {
      continue;
    }
    node.setAttribute(name, value === true ? "" : String(value));
  }
  for (const [name, value] of Object.entries(settings.dataset || {})) {
    if (value === undefined || value === null) {
      continue;
    }
    node.dataset[name] = String(value);
  }

  appendChildren(node, children);
  return node;
}

function appendChildren(node, children) {
  for (const child of Array.isArray(children) ? children : [children]) {
    if (child === null || child === undefined || child === false || child === "") {
      continue;
    }
    node.append(child);
  }
  return node;
}

function replaceChildren(node, children) {
  node.replaceChildren();
  return appendChildren(node, children);
}

// Constant markup only: icons and filter definitions that carry no data.
function setStaticMarkup(node, markup) {
  node.innerHTML = markup;
  return node;
}
