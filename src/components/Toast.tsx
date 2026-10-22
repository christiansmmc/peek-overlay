/** Transient message at the bottom of the window (2.6 s, driven by App). */
export function Toast({ text }: { text: string }) {
  return (
    <div class="toast" role="status">
      {text}
    </div>
  );
}
