import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { IslandApprovalRule } from '../../types';

interface ApprovalRulesPanelProps {
  onBack: () => void;
}

/**
 * Panel showing saved approval rules (Allow Always / Deny Always).
 * Accessible from the expanded session list via a gear icon.
 */
export function ApprovalRulesPanel({ onBack }: ApprovalRulesPanelProps) {
  const [rules, setRules] = useState<IslandApprovalRule[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchRules = useCallback(async () => {
    try {
      const result = await invoke<IslandApprovalRule[]>('get_approval_rules');
      setRules(result);
    } catch {
      // Best-effort
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchRules();
  }, [fetchRules]);

  const handleRemove = async (index: number) => {
    try {
      await invoke('remove_approval_rule', { index });
      fetchRules();
    } catch {
      // Best-effort
    }
  };

  const handleClearAll = async () => {
    try {
      await invoke('clear_approval_rules');
      setRules([]);
    } catch {
      // Best-effort
    }
  };

  return (
    <div className="island-rules">
      <div className="island-rules__header">
        <button
          type="button"
          className="island-rules__back"
          onClick={onBack}
          aria-label="Back to sessions"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M10 3L5 8L10 13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
        <span className="island-rules__title">Approval Rules</span>
        {rules.length > 0 && (
          <button
            type="button"
            className="island-rules__clear"
            onClick={handleClearAll}
          >
            Clear All
          </button>
        )}
      </div>
      <div className="island-rules__body">
        {loading ? (
          <div className="island-rules__empty">Loading...</div>
        ) : rules.length === 0 ? (
          <div className="island-rules__empty">
            No saved rules. Use "Always" when approving a permission to create one.
          </div>
        ) : (
          rules.map((rule, index) => (
            <div key={`${rule.provider_id}-${rule.tool_name}-${index}`} className="island-rules__item">
              <div className="island-rules__item-info">
                <span className={`island-rules__policy island-rules__policy--${rule.policy.toLowerCase()}`}>
                  {rule.policy === 'AllowAlways' ? 'Allow' : 'Deny'}
                </span>
                <span className="island-rules__tool">{rule.tool_name}</span>
                {rule.input_signature && (
                  <span className="island-rules__signature" title={rule.input_signature}>
                    {rule.input_signature.length > 40
                      ? rule.input_signature.slice(0, 40) + '...'
                      : rule.input_signature}
                  </span>
                )}
              </div>
              <button
                type="button"
                className="island-rules__remove"
                onClick={() => handleRemove(index)}
                aria-label={`Remove rule for ${rule.tool_name}`}
              >
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                  <path d="M3 3L9 9M9 3L3 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
                </svg>
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
