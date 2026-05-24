function validatorSourceModeButton(mode, label, options = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "validator-source-mode-button";
  button.textContent = label;
  button.title = mode === "meta" ? "Show TON source metadata" : "Show TON source address";
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
  button.title = type === "ever" ? "Show raw workchain:hash address" : "Show TON user-friendly base64 address";
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

function validatorSourceCell(validator, options = {}) {
  const cell = document.createElement("div");
  const sourceKind = validatorSourceKind(validator, options);
  cell.className = `validator-cell validator-source is-${sourceKind}`;

  const source = validator && validator.source;
  if (source && source.address) {
    const formatted = formatDisplayAddress(source.address, options);
    if (shouldDisplayTonSourceMetadata(validator, options)) {
      const meta = tonSourceMetadata(validator);
      const metadata = copyableValue(
        meta.label,
        formatted.value,
        `validator-source-address validator-source-meta is-${meta.className}`,
        "validator source address"
      );
      const contractHash = source.contract_type_hash || validator?.contract_type_hash;
      setValidatorTooltip(metadata, validatorSourceMetadataTooltipLines(validator, meta, contractHash));
      cell.appendChild(metadata);
      return cell;
    }

    const address = copyableValue(
      formatted.text,
      formatted.value,
      "validator-source-address",
      "validator source address"
    );
    const contractHash = source.contract_type_hash || validator?.contract_type_hash;
    setValidatorTooltip(address, validatorSourceTooltipLines(validator, contractHash));
    cell.appendChild(address);
    return cell;
  }

  if (isDirectValidatorContract(validator)) {
    const direct = document.createElement("span");
    direct.className = "validator-source-direct";
    direct.textContent = "Direct";
    setValidatorTooltip(direct, directValidatorTooltipLines(validator));
    cell.appendChild(direct);
    return cell;
  }

  const tonHash = tonValidatorContractHash(validator, options);
  if (tonHash) {
    const hash = copyableValue(
      shortenContractHash(tonHash),
      tonHash,
      "validator-source-address",
      "validator contract repr hash"
    );
    setValidatorTooltip(hash, `Contract HASH: ${tonHash}`);
    cell.appendChild(hash);
    return cell;
  }

  const unknown = document.createElement("span");
  unknown.className = "validator-source-unknown";
  unknown.textContent = "Unknown";
  cell.appendChild(unknown);
  return cell;
}

function validatorSourceKind(validator, options = {}) {
  const source = validator && validator.source;
  if (source && source.address) {
    return "detail";
  }
  if (isDirectValidatorContract(validator)) {
    return "direct";
  }
  if (tonValidatorContractHash(validator, options)) {
    return "detail";
  }
  return "unknown";
}

function shouldDisplayTonSourceMetadata(validator, options = {}) {
  return options.chainId === "ton" && options.sourceDisplayMode !== "addr" && Boolean(validator?.source?.address);
}

function tonSourceMetadata(validator) {
  const sourceAddress = normalizeSourceAddress(validator?.source?.address);
  const explicit = TON_SOURCE_METADATA_BY_ADDRESS[sourceAddress];
  if (explicit) {
    return {
      className: sourceMetadataClass(explicit.label),
      ...explicit,
    };
  }

  if (validator?.contract_type === "WhalesPoolProxy") {
    return {
      label: "TON Whales",
      name: "TON Whales",
      detail: "Nominator pool source for a TON Whales masterchain proxy.",
      className: "whales",
    };
  }
  if (validator?.contract_type === "HipoValidatorProxy") {
    return {
      label: "Hipo",
      name: "Hipo Finance",
      detail: "Hipo liquid-staking treasury source.",
      className: "hipo",
    };
  }
  if (validator?.contract_type === "ValidatorController") {
    return {
      label: "LST Pool",
      name: "Liquid staking pool",
      detail: "tonstake_pool source. No public owner name was found for this address.",
      className: "lst",
    };
  }
  if (validator?.contract_type === "TonNominatorPool") {
    return {
      label: "Nominator",
      name: "Nominator pool validator",
      detail: "Validator address configured in a TON nominator pool.",
      className: "nominator",
    };
  }
  if (validator?.contract_type === "TonSingleNominatorPool") {
    return {
      label: "Owner",
      name: "Single nominator pool owner",
      detail: "Owner address configured in a TON single nominator pool.",
      className: "owner",
    };
  }
  if (validator?.contract_type === "SingleNominatorV1_0" || validator?.contract_type === "SingleNominatorV1_1") {
    return {
      label: "Owner",
      name: "Single nominator owner",
      detail: "Owner wallet configured in a TON Single Nominator contract.",
      className: "owner",
    };
  }
  if (isDirectValidatorContract(validator)) {
    return {
      label: "Direct",
      name: "Direct validation wallet",
      detail: "Validator wallet participates directly through Elector.",
      className: "direct",
    };
  }
  return {
    label: "Unknown",
    name: "Unknown source owner",
    detail: "No source metadata is known for this address yet.",
    className: "unknown",
  };
}

function normalizeSourceAddress(address) {
  return formatMasterchainAddress(address || "").toLowerCase();
}

function sourceMetadataClass(label) {
  return String(label || "unknown")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "unknown";
}

function validatorSourceTooltipLines(validator, contractHash = "") {
  const role = validatorSourceRole(validator);
  const lines = [];
  if (contractHash) {
    lines.push(`Contract HASH: ${contractHash}`);
  }
  if (role) {
    lines.push(`Source: ${role}`);
  }
  return lines;
}

function validatorSourceMetadataTooltipLines(validator, meta, contractHash = "") {
  const role = validatorSourceRole(validator);
  const lines = role ? [`Source: ${role}`] : [];
  lines.push(`Owner: ${meta.name || meta.label}`);
  if (meta.detail) {
    lines.push(`Metadata: ${meta.detail}`);
  }
  if (contractHash) {
    lines.push(`Contract HASH: ${contractHash}`);
  }
  return lines;
}

function validatorSourceRole(validator) {
  if (!validator) {
    return "";
  }
  if (validator.contract_type === "TonNominatorPool") {
    return "Validator address";
  }
  if (validator.contract_type === "ValidatorController") {
    return "Liquid staking pool";
  }
  if (validator.contract_type === "WhalesPoolProxy") {
    return "Whales pool";
  }
  if (validator.contract_type === "HipoValidatorProxy") {
    return "Hipo Treasury";
  }
  if (validator.contract_type === "DePoolProxy") {
    return "DePool address";
  }
  if (validator.contract_type === "StEverDePoolProxy") {
    return "Staked EVER DePool address";
  }
  if (validator.contract_type === "SingleNominatorV1_0" || validator.contract_type === "SingleNominatorV1_1" || validator.contract_type === "TonSingleNominatorPool") {
    return "Owner address";
  }
  return "";
}

function isDirectValidatorContract(validator) {
  return (
    validator?.contract_type === "EverWallet"
    || validator?.contract_type === "TonWalletV1R3"
    || validator?.contract_type === "TonVestingWallet"
  );
}

function directValidatorTooltipLines(validator) {
  const lines = ["Source: Direct validation wallet"];
  if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
  }
  return lines;
}

function tonValidatorContractHash(validator, options = {}) {
  if (options.chainId !== "ton") {
    return "";
  }
  if (validator?.contract_type && validator.contract_type !== "Unknown") {
    return "";
  }
  return validator?.contract_type_hash || "";
}

function shortenContractHash(hash) {
  return hash && hash.length > 12 ? `${hash.slice(0, 6)}...${hash.slice(-6)}` : (hash || "-");
}
