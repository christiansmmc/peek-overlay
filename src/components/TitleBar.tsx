import { api } from '../ipc';

/** Window title bar drawn by the page: drags the window, minimizes and closes it. */
export function TitleBar() {
  return (
    // "deep": the whole bar drags, except the buttons (Tauri skips clickable elements).
    <div class="titlebar" data-tauri-drag-region="deep">
      <div class="titlebar-brand">
        <span class="titlebar-mark" />
        <span class="titlebar-title">Overlay de desempenho</span>
      </div>
      <button class="tb-btn" title="Minimizar" onClick={() => api.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path d="M0 5.5h10" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button class="tb-btn close" title="Fechar (continua na bandeja)" onClick={() => api.close()}>
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
    </div>
  );
}
