interface Props<T extends string> {
  label: string;
  options: [T, string][];
  value: T;
  onChange(v: T): void;
}

/** Labeled segmented control ("Estilo": Linha / Card). */
export function Segmented<T extends string>({ label, options, value, onChange }: Props<T>) {
  return (
    <div class="segmented">
      <span class="section-label">{label}</span>
      <div class="seg">
        {options.map(([v, text]) => (
          <button key={v} class={v === value ? 'seg-btn on' : 'seg-btn'} onClick={() => onChange(v)}>
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}
