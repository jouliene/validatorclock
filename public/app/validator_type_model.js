function validatorSourceType(hash) {
  return VALIDATOR_SOURCE_TYPES[String(hash || "").toLowerCase()] || UNKNOWN_VALIDATOR_TYPE;
}

function validatorContractType(typeName) {
  return VALIDATOR_CONTRACT_TYPES[typeName] || UNKNOWN_VALIDATOR_TYPE;
}

function displayedValidatorType(validator) {
  const hash = validator && validator.source && validator.source.contract_type_hash;
  if (hash) {
    return { ...validatorSourceType(hash), hash };
  }
  return { ...validatorContractType(validator && validator.contract_type), hash: "" };
}

function validatorGlossaryLabelsForSnapshot(snapshot) {
  const labels = new Set();
  collectValidatorGlossaryLabels(labels, snapshot?.current_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.previous_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.next_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.election?.candidates);
  collectValidatorGlossaryLabels(labels, snapshot?.current_set?.recent_absent_validators);
  collectValidatorGlossaryLabels(labels, snapshot?.previous_set?.recent_absent_validators);
  collectValidatorGlossaryLabels(labels, snapshot?.next_set?.recent_absent_validators);
  return labels;
}

function collectValidatorGlossaryLabels(labels, validators) {
  if (!Array.isArray(validators)) {
    return;
  }
  for (const validator of validators) {
    labels.add(displayedValidatorType(validator).label);
  }
}

function validatorTypeGlossaryEntry(label) {
  return VALIDATOR_TYPE_GLOSSARY.find((entry) => entry.label === label);
}

function validatorContractDisplayName(validator) {
  const typeName = validator?.contract_type || "";
  if (typeName && typeName !== "Unknown") {
    return VALIDATOR_CONTRACT_TYPE_NAMES[typeName] || typeName;
  }

  const displayed = displayedValidatorType(validator);
  const entry = validatorTypeGlossaryEntry(displayed.label);
  return entry?.name || "Unknown";
}
