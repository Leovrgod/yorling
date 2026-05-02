interface ToggleProps {
  active: boolean;
  onChange: (active: boolean) => void;
  label?: string;
  disabled?: boolean;
}

export function Toggle({ active, onChange, label, disabled }: ToggleProps) {
  return (
    <button
      className={`toggle ${active ? 'active' : ''}`}
      onClick={() => !disabled && onChange(!active)}
      disabled={disabled}
      type="button"
    >
      <span className="toggle-track" />
      {label && <span className="toggle-label">{label}</span>}
    </button>
  );
}
