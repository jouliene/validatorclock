function validatorSourceTooltipLines(validator, source = null) {
  const role = validatorSourceRole(validator);
  const sourceType = sourceContractDisplayName(source);
  const sourceHash = source?.contract_type_hash || "";
  const lines = [];
  if (sourceType) {
    lines.push(`Source type: ${sourceType}`);
  }
  if (sourceHash) {
    lines.push(`Source HASH: ${sourceHash}`);
  } else if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
  }
  if (role) {
    lines.push(`Source: ${role}`);
  }
  return lines;
}

function validatorSourceMetadataTooltipLines(validator, meta, source = null) {
  const role = validatorSourceRole(validator);
  const sourceType = sourceContractDisplayName(source);
  const sourceHash = source?.contract_type_hash || "";
  const lines = role ? [`Source: ${role}`] : [];
  lines.push(`Owner: ${meta.name || meta.label}`);
  if (meta.detail) {
    lines.push(`Metadata: ${meta.detail}`);
  }
  if (sourceType) {
    lines.push(`Source type: ${sourceType}`);
  }
  if (sourceHash) {
    lines.push(`Source HASH: ${sourceHash}`);
  } else if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
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
  if (validator.contract_type === "StEverStrategy") {
    return "Strategy controller";
  }
  if (validator.contract_type === "SingleNominatorV1_0" || validator.contract_type === "SingleNominatorV1_1" || validator.contract_type === "TonSingleNominatorPool") {
    return "Owner address";
  }
  return "";
}

function sourceContractDisplayName(source) {
  const typeName = source?.contract_type || "";
  return typeName ? (VALIDATOR_CONTRACT_TYPE_NAMES[typeName] || typeName) : "";
}

function directValidatorTooltipLines(validator) {
  const lines = ["Source: Direct validation wallet"];
  if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
  }
  return lines;
}
