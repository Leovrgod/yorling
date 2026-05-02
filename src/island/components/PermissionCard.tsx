import { useState } from 'react';
import type { IslandPendingPermission } from '../../types';
import { useIslandActions } from '../hooks/useIslandState';
import { t } from '../i18n';
import { formatToolLabel } from '../presentation';

interface PermissionCardProps {
  permission: IslandPendingPermission;
}

/**
 * Permission approval card with Allow/Deny/Always Allow buttons.
 */
export function PermissionCard({ permission }: PermissionCardProps) {
  const { approvePermission } = useIslandActions();
  const [loading, setLoading] = useState(false);
  const [expired, setExpired] = useState(false);

  const handleDecision = async (decision: 'allow' | 'deny' | 'allow_always') => {
    if (expired) return;
    setLoading(true);
    try {
      await approvePermission(permission.request_id, decision);
    } catch {
      // The approval request has expired (bridge connection timed out).
      setExpired(true);
    }
    setLoading(false);
  };

  // Format the input for display
  const inputPreview = formatPermissionInput(permission.tool, permission.input);
  const toolLabel = formatToolLabel(permission.tool, permission.input) ?? t('tool.unknown');

  return (
    <div className={`island-permission${expired ? ' island-permission--expired' : ''}`} role="dialog" aria-label={t('collapsed.approval_needed')}>
      <div className="island-permission__header">
        <span className="island-permission__icon" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
            <path d="M7 5V8M7 10V10.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
          </svg>
        </span>
        <span className="island-permission__tool">{toolLabel}</span>
      </div>

      {inputPreview && (
        <div className="island-permission__preview">
          <code>{inputPreview}</code>
        </div>
      )}

      {expired ? (
        <div className="island-permission__expired-notice">
          {t('permission.expired')}
        </div>
      ) : (
        <div className="island-permission__actions">
          <button
            type="button"
            className="island-permission__btn island-permission__btn--deny"
            onClick={() => handleDecision('deny')}
            disabled={loading}
          >
            {t('permission.deny')}
          </button>
          <button
            type="button"
            className="island-permission__btn island-permission__btn--allow"
            onClick={() => handleDecision('allow')}
            disabled={loading}
          >
            {t('permission.allow')}
          </button>
          <button
            type="button"
            className="island-permission__btn island-permission__btn--always"
            onClick={() => handleDecision('allow_always')}
            disabled={loading}
          >
            {t('permission.always')}
          </button>
        </div>
      )}
    </div>
  );
}

function formatPermissionInput(tool: string, input: Record<string, unknown>): string {
  const normalizedTool = tool.trim().toLowerCase();

  if (
    (normalizedTool === 'bash' || normalizedTool === 'shell' || normalizedTool.endsWith('_shell')) &&
    typeof input.command === 'string'
  ) {
    return input.command.length > 120
      ? input.command.slice(0, 120) + '...'
      : input.command;
  }
  if (
    normalizedTool === 'edit' ||
    normalizedTool === 'write' ||
    normalizedTool === 'file_edit'
  ) {
    const path = input.file_path ?? input.path;
    if (typeof path === 'string') return path;
  }
  if (
    (normalizedTool === 'read' || normalizedTool === 'read_file') &&
    typeof input.file_path === 'string'
  ) {
    return input.file_path;
  }
  const json = JSON.stringify(input);
  return json.length > 150 ? json.slice(0, 150) + '...' : json;
}
