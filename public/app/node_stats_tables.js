// Node location statistics: the cards, tables, and ranking elements.
function nodeStatsCard(label, value, detail, tooltip = "", featured = false, extraClass = "") {
  const className = ["node-stats-card", featured ? "is-featured" : "", extraClass]
    .filter(Boolean)
    .join(" ");
  const card = el("div", { className }, [
    el("span", { text: label }),
    el("strong", { text: value }),
    detail ? el("small", { text: detail }) : null,
  ]);
  if (tooltip) {
    setValidatorTooltip(card, tooltip);
  }
  return card;
}

function hideNodeStatsTooltip() {
  if (typeof hideValidatorTooltip === "function") {
    hideValidatorTooltip();
  }
}

function wireNodeStatsTableScrollHints(root) {
  const shells = Array.from(root.querySelectorAll(".node-stats-table-shell"));
  const updateShell = (shell) => {
    const maxScrollLeft = Math.max(0, shell.scrollWidth - shell.clientWidth);
    const hasMore = maxScrollLeft > 1 && shell.scrollLeft < maxScrollLeft - 1;
    shell.classList.toggle("has-scroll-more", hasMore);
  };

  for (const shell of shells) {
    shell.addEventListener("scroll", () => updateShell(shell), { passive: true });
    window.requestAnimationFrame(() => updateShell(shell));
  }
}

function nodeStatsBlockTitle(label, icon) {
  return el("h3", "node-stats-block-title", [
    setStaticMarkup(
      el("span", {
        className: `node-stats-block-icon node-stats-icon-${icon}`,
        attrs: { "aria-hidden": "true" },
      }),
      nodeStatsBlockIconSvg(icon),
    ),
    el("span", { text: label }),
  ]);
}

function nodeStatsBlockIconSvg(icon) {
  if (icon === "countries") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="12" cy="12" r="8.2"></circle>
        <path d="M3.8 12h16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 1 0 16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 0 0 16.4"></path>
      </svg>
    `;
  }
  if (icon === "isp") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="7" cy="8" r="2"></circle>
        <circle cx="17" cy="8" r="2"></circle>
        <circle cx="12" cy="17" r="2"></circle>
        <path d="M8.7 9.2 11 15.1"></path>
        <path d="m15.3 9.2-2.3 5.9"></path>
        <path d="M9 8h6"></path>
      </svg>
    `;
  }
  if (icon === "city") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <path d="M5 19V7l5-2v14"></path>
        <path d="M10 19V9l5-2v12"></path>
        <path d="M15 19v-7l4 1.8V19"></path>
        <path d="M4 19h16"></path>
      </svg>
    `;
  }
  return `
    <svg viewBox="0 0 24 24" focusable="false">
      <path d="M8 21h8"></path>
      <path d="M12 17v4"></path>
      <path d="M7 4h10v3a5 5 0 0 1-10 0Z"></path>
      <path d="M7 6H4a3 3 0 0 0 3 3"></path>
      <path d="M17 6h3a3 3 0 0 1-3 3"></path>
    </svg>
  `;
}

function wireNodeStatsRankingToggle(root) {
  const button = root.querySelector("[data-node-stats-ranking-toggle]");
  if (!button) {
    return;
  }

  button.addEventListener("click", () => {
    const block = button.closest(".node-stats-block-placement");
    if (!block) {
      return;
    }
    const expanded = block.classList.toggle("is-ranking-expanded");
    state.nodeStatsLocationRankingExpanded = expanded;
    const summary = block.querySelector("[data-node-stats-ranking-summary]");
    button.textContent = nodeStatsRankingActionText(expanded);
    button.setAttribute("aria-expanded", expanded ? "true" : "false");
    if (summary) {
      summary.textContent = nodeStatsRankingSummaryText(
        Number(summary.dataset.visibleCount || 0),
        Number(summary.dataset.totalCount || 0),
        expanded,
      );
    }
  });
}

function nodeStatsCountryTable(rows) {
  return nodeStatsAggregateTable(rows, NODE_STATS_LABELS.columns.country, "country", "countries");
}

function nodeStatsRankTable(rows) {
  return nodeStatsAggregateTable(rows, NODE_STATS_LABELS.columns.cluster, "cluster", "clusters");
}

function nodeStatsAggregateTable(rows, nameHeader, singularLabel, pluralLabel) {
  const tableRows = nodeStatsVisibleRows(rows, singularLabel, pluralLabel);
  return nodeStatsTableShell(
    "node-stats-table",
    ["rank", "name", "count", "stake", "percent"],
    [
      NODE_STATS_LABELS.columns.rank,
      nameHeader,
      NODE_STATS_LABELS.columns.nodes,
      NODE_STATS_LABELS.columns.stake,
      NODE_STATS_LABELS.columns.weightPercent,
    ],
    tableRows.map((row, index) =>
      el("tr", { className: row.isRemainder ? "is-remainder" : "" }, [
        el("td", { text: formatNodeStatsInteger(index + 1) }),
        el("td", { text: row.label }),
        el("td", { text: formatNodeStatsInteger(row.nodes) }),
        el("td", { text: formatNodeStatsStake(row.stake) }),
        el("td", { text: formatPercent(row.weightPercent) }),
      ]),
    ),
  );
}

function nodeStatsTableShell(tableClass, columnKeys, headers, rows) {
  return el("div", "node-stats-table-shell", [
    el("table", tableClass, [
      el(
        "colgroup",
        {},
        columnKeys.map((key) => el("col", `node-stats-col-${key}`)),
      ),
      el("thead", {}, [
        el(
          "tr",
          {},
          headers.map((header) => el("th", { text: header, attrs: { scope: "col" } })),
        ),
      ]),
      el("tbody", {}, rows),
    ]),
  ]);
}

function nodeStatsVisibleRows(rows, singularLabel, pluralLabel) {
  const visibleRows = rows.slice(0, NODE_STATS_VISIBLE_ROWS);
  const remainderRows = rows.slice(NODE_STATS_VISIBLE_ROWS);
  if (!remainderRows.length) {
    return visibleRows;
  }

  const label = remainderRows.length === 1 ? singularLabel : pluralLabel;
  return [
    ...visibleRows,
    {
      label: `Other ${formatNodeStatsInteger(remainderRows.length)} ${label}`,
      nodes: remainderRows.reduce((sum, row) => sum + row.nodes, 0),
      stake: remainderRows.reduce((sum, row) => sum + row.stake, 0),
      stakePercent: remainderRows.reduce((sum, row) => sum + row.stakePercent, 0),
      weightPercent: remainderRows.reduce((sum, row) => sum + row.weightPercent, 0),
      isRemainder: true,
    },
  ];
}

function nodeStatsPlacement(stats) {
  const mappedLocations = stats.mappedLocationRows;
  const visibleCount = Math.min(5, mappedLocations.length);
  const extraCount = Math.max(0, mappedLocations.length - visibleCount);
  const expanded = isNodeStatsLocationRankingExpanded();

  const table = nodeStatsTableShell(
    "node-stats-table node-stats-ranking-table",
    ["rank", "location", "distance-primary", "distance-secondary", "distance-tertiary"],
    [
      NODE_STATS_LABELS.columns.rank,
      NODE_STATS_LABELS.columns.mappedLocation,
      NODE_STATS_LABELS.columns.weightedAverage,
      NODE_STATS_LABELS.columns.median,
      NODE_STATS_LABELS.columns.p90,
    ],
    mappedLocations.map((row, index) =>
      el("tr", { className: nodeStatsPlacementRowClass(index, visibleCount) }, [
        el("td", { text: formatNodeStatsInteger(index + 1) }),
        el("td", { text: row.label }),
        el("td", { text: formatNodeStatsDistance(row.weightedAverageKm) }),
        el("td", { text: formatNodeStatsDistance(row.medianKm) }),
        el("td", { text: formatNodeStatsDistance(row.p90Km) }),
      ]),
    ),
  );

  return el("div", "node-stats-placement", [
    table,
    extraCount ? nodeStatsRankingFooter(visibleCount, mappedLocations.length, expanded) : null,
  ]);
}

function nodeStatsRankingFooter(visibleCount, totalCount, expanded) {
  return el("div", "node-stats-ranking-footer", [
    el("span", {
      text: nodeStatsRankingSummaryText(visibleCount, totalCount, expanded),
      attrs: { "data-node-stats-ranking-summary": true },
      dataset: { visibleCount, totalCount },
    }),
    el("button", {
      className: "node-stats-ranking-action",
      text: nodeStatsRankingActionText(expanded),
      attrs: {
        type: "button",
        "data-node-stats-ranking-toggle": true,
        "aria-expanded": expanded ? "true" : "false",
      },
    }),
  ]);
}

function nodeStatsPlacementBlockClass() {
  return [
    "node-stats-block",
    "node-stats-block-placement",
    isNodeStatsLocationRankingExpanded() ? "is-ranking-expanded" : "",
  ]
    .filter(Boolean)
    .join(" ");
}

function isNodeStatsLocationRankingExpanded() {
  return Boolean(state.nodeStatsLocationRankingExpanded);
}

function nodeStatsRankingActionText(expanded) {
  return expanded
    ? NODE_STATS_LABELS.actions.showTopFive
    : NODE_STATS_LABELS.actions.viewFullRanking;
}

function nodeStatsRankingSummaryText(visibleCount, totalCount, expanded) {
  const visible = Number.isFinite(visibleCount) ? Math.max(0, Math.trunc(visibleCount)) : 0;
  const total = Number.isFinite(totalCount) ? Math.max(0, Math.trunc(totalCount)) : 0;
  if (expanded) {
    return `Showing all ${formatNodeStatsInteger(total)}`;
  }
  return `Top ${formatNodeStatsInteger(visible)} of ${formatNodeStatsInteger(total)}`;
}

function nodeStatsPlacementRowClass(index, visibleCount) {
  const classes = [];
  if (index === 0) {
    classes.push("is-best");
  }
  if (index >= visibleCount) {
    classes.push("is-extra-ranking");
  }
  return classes.join(" ");
}

function formatNodeStatsRoundColor(value) {
  const color = String(value || "").trim();
  return color ? `${color.slice(0, 1).toUpperCase()}${color.slice(1).toLowerCase()}` : "";
}

function formatNodeStatsStake(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatNodeStatsInteger(value) {
  return Number(value || 0).toLocaleString(undefined, { maximumFractionDigits: 0 });
}

function formatNodeStatsDistance(value) {
  return `${Math.round(Number(value || 0)).toLocaleString()} km`;
}
