function validatorHeader(labels, options = {}) {
  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of labels) {
    header.appendChild(validatorHeaderCell(label, options));
  }
  return header;
}

function validatorHeaderCell(label, options = {}) {
  const cell = document.createElement("div");
  const classes = ["validator-cell"];
  const semanticClass = VALIDATOR_HEADER_CLASSES[label];
  if (semanticClass) {
    classes.push(semanticClass);
  }
  if (VALIDATOR_NUMBER_HEADERS.has(label)) {
    classes.push("validator-number");
  }
  cell.className = classes.join(" ");

  if (label === "Type") {
    return validatorTypeHeaderCell(cell, options);
  }
  if (label === "Source" && options.chainId === "ton") {
    return validatorSourceHeaderCell(cell, options);
  }
  if (label === "Validator" && options.chainId === "ton") {
    return validatorAddressHeaderCell(cell, options);
  }
  if (label === "History") {
    return validatorHistoryHeaderCell(cell);
  }

  cell.textContent = label;
  return cell;
}

function validatorTypeHeaderCell(cell, options) {
  cell.classList.add("validator-type-heading");
  const name = document.createElement("span");
  name.textContent = "Type";
  const help = document.createElement("button");
  help.type = "button";
  help.className = "validator-type-help";
  help.setAttribute("aria-label", "Show type glossary");
  help.setAttribute("aria-expanded", "false");
  help.validatorGlossaryLabels = normalizedGlossaryLabels(options.glossaryLabels);
  setValidatorTooltip(help, "Type glossary");
  help.innerHTML = `
    <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
      <circle cx="12" cy="12" r="8.5"></circle>
      <path d="M12 11.5v5"></path>
      <path d="M12 8h.01"></path>
    </svg>
  `;
  help.addEventListener("click", (event) => {
    event.stopPropagation();
    toggleValidatorTypeGlossary(help);
  });
  cell.append(name, help);
  return cell;
}

function validatorSourceHeaderCell(cell, options) {
  cell.classList.add("validator-source-heading");
  const name = document.createElement("span");
  name.textContent = "Source";
  const toggle = document.createElement("span");
  toggle.className = "validator-source-mode-toggle";
  toggle.setAttribute("role", "group");
  toggle.setAttribute("aria-label", "Source display");
  toggle.append(
    validatorSourceModeButton("meta", "META", options),
    validatorSourceModeButton("addr", "ADDR", options)
  );
  cell.append(name, toggle);
  return cell;
}

function validatorAddressHeaderCell(cell, options) {
  cell.classList.add("validator-address-heading");
  const name = document.createElement("span");
  name.textContent = "Validator";
  const toggle = document.createElement("span");
  toggle.className = "validator-source-mode-toggle validator-address-mode-toggle";
  toggle.setAttribute("role", "group");
  toggle.setAttribute("aria-label", "Validator address type");
  toggle.append(
    validatorAddressModeButton("ever", "HASH", options),
    validatorAddressModeButton("ton", "BASE64", options)
  );
  cell.append(name, toggle);
  return cell;
}

function validatorHistoryHeaderCell(cell) {
  cell.classList.add("validator-history-heading");
  const name = document.createElement("span");
  name.textContent = "History";
  const direction = document.createElement("span");
  direction.className = "history-direction";
  direction.textContent = "Older -> Latest";
  cell.append(name, direction);
  return cell;
}
