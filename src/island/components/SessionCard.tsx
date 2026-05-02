import type { IslandPluginInfo, IslandSession } from '../../types';
import { t } from '../i18n';
import {
  formatElapsedDuration,
  formatProviderLabel,
  getPhaseLabel,
  getSessionProjectLabel,
  getSessionActivity,
  getSessionSummary,
  getSessionTitle,
  getLatestTranscriptActivityTimestamp,
  getVisibleToolLabels,
  isGenericSessionHeading,
} from '../presentation';
import { MascotView } from './MascotView';
import { PermissionCard } from './PermissionCard';
import { QuestionCard } from './QuestionCard';
import { useIslandActions } from '../hooks/useIslandState';

interface SessionCardProps {
  session: IslandSession;
  plugins?: IslandPluginInfo[];
  onSelect?: () => void;
}

const HIDDEN_BUILTIN_SESSION_FOOTER_SLOT_IDS = new Set([
  'core.transcript-preview.status',
  'core.transcript-preview.session-id',
  'core.tool-history.latest',
]);

/**
 * A single session's status card in the expanded view.
 * Shows only the agent block header + active command badges + permission/question cards.
 * Full conversation/tool history is shown in ChatView on click.
 */
export function SessionCard({ session, plugins = [], onSelect }: SessionCardProps) {
  const phaseLabel = getPhaseLabel(session.phase);
  const phaseClass = `island-session__phase island-session__phase--${session.phase}`;
  const providerLabel = formatProviderLabel(session.provider_id);
  const { jumpToTerminal } = useIslandActions();
  const projectLabel = getSessionProjectLabel(session);
  const titleLabel = getSessionTitle(session);
  const hidesRedundantProviderTitle = isGenericSessionHeading(titleLabel);
  const showTitleLabel = !hidesRedundantProviderTitle;
  const showTitleLine = Boolean(projectLabel) || showTitleLabel;
  const summaryLabel = getSessionSummary(session);
  const activity = getSessionActivity(session);
  const badgeTimestamp = getLatestTranscriptActivityTimestamp(session) ?? session.started_at;
  const visibleToolLabels = getVisibleToolLabels(session);
  const sessionFooterSlots = plugins.flatMap((plugin) =>
    plugin.slots
      .filter((slot) =>
        slot.slot === 'session_footer'
        && !HIDDEN_BUILTIN_SESSION_FOOTER_SLOT_IDS.has(slot.id)
        && (slot.provider_ids.length === 0 || slot.provider_ids.includes(session.provider_id)))
      .map((slot) => ({ plugin, slot })),
  );

  return (
    <div className="island-session">
      <div
        className="island-session__header"
        onClick={onSelect}
        role={onSelect ? 'button' : undefined}
        tabIndex={onSelect ? 0 : undefined}
        onKeyDown={onSelect ? (e) => { if (e.key === 'Enter' || e.key === ' ') onSelect(); } : undefined}
        style={onSelect ? { cursor: 'pointer' } : undefined}
      >
        <div className="island-session__provider-group">
          <span className="island-session__provider-dot">
            <MascotView providerId={session.provider_id} phase={session.phase} size={28} />
          </span>
          <div className="island-session__provider-copy">
            {showTitleLine ? (
              <div className="island-session__title-line">
                {projectLabel ? <span className="island-session__project">{projectLabel}</span> : null}
                {projectLabel && showTitleLabel ? (
                  <span className="island-session__title-separator">·</span>
                ) : null}
                {showTitleLabel ? <span className="island-session__title">{titleLabel}</span> : null}
              </div>
            ) : null}
            <span
              className={[
                'island-session__meta',
                hidesRedundantProviderTitle ? 'island-session__meta--expanded' : '',
              ].filter(Boolean).join(' ')}
            >
              {summaryLabel}
            </span>
          </div>
        </div>
        <div className="island-session__header-actions">
          <div className="island-session__badges">
            <span className="island-session__badge island-session__badge--time">
              {formatRelativeTime(badgeTimestamp)}
            </span>
            <span className="island-session__badge island-session__badge--provider">
              {providerLabel}
            </span>
          </div>
          <div className="island-session__header-controls">
            <button
              type="button"
              className="island-session__jump-btn"
              onClick={(event) => {
                event.stopPropagation();
                void jumpToTerminal(session.id);
              }}
              title={t('session.jump')}
            >
              ↗
            </button>
            <span className={phaseClass}>{phaseLabel}</span>
          </div>
        </div>
      </div>

      {visibleToolLabels.length > 0 && (
        <div className="island-session__tools">
          {visibleToolLabels.map((tool, i) => (
            <span key={i} className="island-session__tool-badge">{tool}</span>
          ))}
        </div>
      )}

      {activity ? (
        <div
          className={[
            'island-session__activity',
            activity.isHumanized ? 'island-session__activity--humanized' : '',
            `island-session__activity--${activity.accentToken}`,
          ].filter(Boolean).join(' ')}
        >
          {activity.text}
        </div>
      ) : null}

      {session.pending_permission && (
        <PermissionCard permission={session.pending_permission} />
      )}

      {session.pending_question && (
        <QuestionCard question={session.pending_question} sessionId={session.id} />
      )}

      {sessionFooterSlots.length > 0 ? (
        <div className="island-session__tools">
          {sessionFooterSlots.map(({ plugin, slot }) => (
            <span key={`${plugin.id}:${slot.id}`} className="island-session__tool-badge">
              {slot.title ?? plugin.name}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function formatRelativeTime(timestamp: number): string {
  return formatElapsedDuration(timestamp);
}
