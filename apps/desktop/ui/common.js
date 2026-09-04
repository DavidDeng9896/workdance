const invoke = (cmd, args = {}) => {
  if (window.__TAURI__?.core?.invoke) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  // Browser preview fallback (static server / CI screenshot)
  return Promise.reject(new Error("tauri unavailable"));
};

function toast(msg) {
  let el = document.querySelector(".toast");
  if (!el) {
    el = document.createElement("div");
    el.className = "toast";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add("show");
  clearTimeout(el._t);
  el._t = setTimeout(() => el.classList.remove("show"), 1800);
}

function bindSeg(root, onChange) {
  root.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("click", () => {
      root.querySelectorAll("button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      onChange(btn.dataset.value);
    });
  });
}

function setSeg(root, value) {
  root.querySelectorAll("button").forEach((b) => {
    b.classList.toggle("active", b.dataset.value === value);
  });
}

window.WD = { invoke, toast, bindSeg, setSeg };
