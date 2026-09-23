function startEverscaleMemorial() {
  const dialog = $("everscaleMemorial");
  const image = $("everscaleMemorialImage");
  const close = $("closeEverscaleMemorial");
  if (!dialog || !image || !close) return;

  const show = () => {
    dialog.showModal();
    document.documentElement.classList.add("memorial-open");
  };
  close.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => {
    if (event.target !== dialog) return;
    const bounds = dialog.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right
      || event.clientY < bounds.top || event.clientY > bounds.bottom) {
      dialog.close();
    }
  });
  dialog.addEventListener("close", () => {
    document.documentElement.classList.remove("memorial-open");
    document.querySelector(".chain-tab[aria-current]")?.focus({ preventScroll: true });
  });

  // Show once per page load, only when the artwork is ready.
  if (image.complete && image.naturalWidth) show();
  else image.addEventListener("load", show, { once: true });
}
