function validatorSourceModeButton(mode, label, options = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "validator-source-mode-button";
  button.textContent = label;
  setValidatorTooltip(button, mode === "meta" ? "Show TON source metadata" : "Show TON source address");
  button.setAttribute("aria-pressed", String((options.sourceDisplayMode || "meta") === mode));
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (typeof options.onSourceDisplayModeChange === "function") {
      options.onSourceDisplayModeChange(mode);
    }
  });
  return button;
}

function validatorAddressModeButton(type, label, options = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "validator-source-mode-button validator-address-mode-button";
  button.textContent = label;
  setValidatorTooltip(
    button,
    type === "ever" ? "Show raw workchain:hash address" : "Show TON user-friendly base64 address",
  );
  button.setAttribute("aria-pressed", String((options.addressType || "ton") === type));
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (typeof options.onAddressTypeChange === "function") {
      options.onAddressTypeChange(type);
    }
  });
  return button;
}
