import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';

export function MusicPlaceholder() {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language);

  return (
    <div className="page-enter">
      <div style={{ marginBottom: 'var(--space-8)' }}>
        <h2>{copy.music.title}</h2>
        <p className="text-sm text-secondary" style={{ marginTop: 'var(--space-1)' }}>
          {copy.music.subtitle}
        </p>
      </div>

      <div className="card">
        <div className="card-body" style={{ padding: 'var(--space-12)', textAlign: 'center' }}>
          <div style={{ fontSize: '48px', marginBottom: 'var(--space-4)', opacity: 0.2 }}>♪</div>
          <div className="text-secondary text-sm">{copy.placeholder.comingSoon}</div>
          <div className="text-tertiary text-xs" style={{ marginTop: 'var(--space-2)' }}>
            {copy.music.description}
          </div>
        </div>
      </div>
    </div>
  );
}
