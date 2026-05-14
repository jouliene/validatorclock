function copyableValue(text, value, className, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `validator-copy ${className}`.trim();
  button.setAttribute("aria-label", `Copy ${label}`);
  const textNode = document.createElement("span");
  textNode.className = "validator-copy-text";
  textNode.textContent = text;
  const feedback = document.createElement("span");
  feedback.className = "validator-copy-feedback";
  feedback.textContent = "Copied";
  button.append(textNode, feedback);
  if (!value || value === "-") {
    button.disabled = true;
    return button;
  }
  wireCopyButton(button, feedback, value);
  return button;
}

function wireCopyButton(button, feedback, value) {
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    hideValidatorTooltip();
    try {
      await copyText(value);
      markCopied(button);
    } catch (error) {
      button.classList.add("is-failed");
      feedback.textContent = "Copy failed";
      window.setTimeout(() => {
        button.classList.remove("is-failed");
        feedback.textContent = "Copied";
      }, 1200);
    }
  });
}

async function copyText(value) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(value);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) {
    throw new Error("copy failed");
  }
}

function markCopied(button) {
  button.classList.add("is-copied");
  if (button.dataset.copyTimer) {
    window.clearTimeout(Number(button.dataset.copyTimer));
  }
  button.dataset.copyTimer = String(window.setTimeout(() => {
    button.classList.remove("is-copied");
    delete button.dataset.copyTimer;
  }, 1000));
}
