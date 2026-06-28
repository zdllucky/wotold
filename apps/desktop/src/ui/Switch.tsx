// [B18.6c] Wotold v2 uikit — toggle switch (.switch from wk.css).

interface SwitchProps {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled }: SwitchProps) {
  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-on={checked ? 'true' : undefined}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}
