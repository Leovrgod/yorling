interface MiniToggleProps {
  active: boolean;
  onChange: (active: boolean) => void;
  className?: string;
  ariaLabel?: string;
  disabled?: boolean;
}

export function MiniToggle({
  active,
  onChange,
  className,
  ariaLabel,
  disabled = false,
}: MiniToggleProps) {
  return (
    <button
      type="button"
      className={['mini-toggle', active ? 'active' : '', className ?? ''].join(' ').trim()}
      onClick={(event) => {
        event.stopPropagation();
        if (!disabled) {
          onChange(!active);
        }
      }}
      aria-label={ariaLabel ?? (active ? 'Disable' : 'Enable')}
      disabled={disabled}
    >
      <span className="mini-toggle-track" />
    </button>
  );
}
