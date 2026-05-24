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

function directValidatorTooltipLines(validator) {
  const lines = ["Source: Direct validation wallet"];
  if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
  }
  return lines;
}
