import { useEffect, useMemo, useState } from 'react';
import type { CSSProperties } from 'react';
import { useKeyboardService } from '../../hooks/useTauriCommand';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import { useKeyboardStore } from '../../stores/keyboardStore';
import { MiniToggle } from '../common/MiniToggle';
import {
  buildKeyboardLayerViews,
  getKeyboardLayoutRows,
  getLocalizedRuleTexts,
  type KeyboardLayerId,
  type KeyboardLayerView,
  type KeyboardLayoutKey,
} from './keyboardUiModel';
import { getKeyboardTextClasses } from './keyboardKeyMetrics';
import { getKeyboardMappingRules } from './keyboardMappingData';
import { detectYorlingPlatform } from '../../utils/platform';

function KeyboardKey({
  keyItem,
  layer,
  holdLabel,
  modeTag,
}: {
  keyItem: KeyboardLayoutKey;
  layer: KeyboardLayerView;
  holdLabel: string;
  modeTag: string;
}) {
  const target = layer.keyTargets[keyItem.id];
  const isModifier = layer.modifierKey === keyItem.id;
  const displayText = target ? target.display : isModifier ? holdLabel : '';
  const descriptionText = target ? target.description : isModifier ? layer.label : '';
  const { displayClassName, descriptionClassName } = getKeyboardTextClasses(displayText, descriptionText);

  return (
    <div
      className={`keyboard-key ${target ? 'mapped' : ''} ${isModifier ? 'modifier' : ''}`}
      style={{ '--key-flex': keyItem.width } as CSSProperties}
    >
      <div className="keyboard-key-top">
        <span className="keyboard-key-label">{keyItem.label}</span>
        {isModifier ? <span className="keyboard-key-tag">{modeTag}</span> : null}
      </div>
      {target ? (
        <div className="keyboard-key-map">
          <span className={displayClassName}>{displayText}</span>
          <span className={descriptionClassName}>{descriptionText}</span>
        </div>
      ) : isModifier ? (
        <div className="keyboard-key-map">
          <span className={displayClassName}>{displayText}</span>
          <span className={descriptionClassName}>{descriptionText}</span>
        </div>
      ) : (
        <div className="keyboard-key-empty" />
      )}
    </div>
  );
}

export function KeyboardMapping() {
  const language = useAppStore((state) => state.language);
  const disabledRules = useAppStore((state) => state.disabledRules);
  const setRulesDisabled = useAppStore((state) => state.setRulesDisabled);
  const copy = getUiCopy(language);
  const { status, rules } = useKeyboardStore();
  const { startInterceptor, setEnabled, openAccessibilitySettings } = useKeyboardService();
  const platform = status.platform === 'unknown' ? detectYorlingPlatform() : status.platform;
  const platformRules = useMemo(
    () => platform === 'windows' ? getKeyboardMappingRules(platform) : rules,
    [platform, rules],
  );
  const keyboardRows = useMemo(() => getKeyboardLayoutRows(platform), [platform]);

  const { layers, otherRules } = useMemo(
    () => buildKeyboardLayerViews(platformRules, language, platform),
    [language, platform, platformRules],
  );
  const [activeLayerId, setActiveLayerId] = useState<KeyboardLayerId>('space');

  useEffect(() => {
    if (!layers.some((layer) => layer.id === activeLayerId) && layers[0]) {
      setActiveLayerId(layers[0].id);
    }
  }, [activeLayerId, layers]);

  const activeLayer = layers.find((layer) => layer.id === activeLayerId) ?? layers[0];
  const otherRuleIds = otherRules.map((rule) => rule.id);
  const areOtherMappingsEnabled = otherRuleIds.every((ruleId) => !disabledRules.includes(ruleId));
  const canStartEngine = status.interception_supported
    && (!status.requires_accessibility || status.has_accessibility);
  const shouldShowAccessibilityAlert = status.platform !== 'unknown'
    && status.requires_accessibility
    && !status.has_accessibility;
  const shouldShowUnsupportedAlert = status.platform !== 'unknown' && !status.interception_supported;

  const handleStartEngine = async () => {
    try {
      await startInterceptor();
    } catch {
      // Error is stored in the global store and shown by ErrorBanner.
    }
  };

  const handleToggleOtherMappings = (nextActive: boolean) => {
    setRulesDisabled(otherRuleIds, !nextActive);
  };

  if (!activeLayer) {
    return null;
  }

  return (
    <div className="page-enter mapping-page">
      <section className="card mapping-hero">
        {!status.running ? (
          <div className="mapping-hero-toolbar">
            <button
              className="btn btn-primary"
              onClick={handleStartEngine}
              disabled={!canStartEngine}
              type="button"
            >
              {copy.keyboard.start}
            </button>
          </div>
        ) : null}

        {shouldShowUnsupportedAlert ? (
          <div className="mapping-alert">
            <div className="mapping-alert-copy">
              <span className="mapping-alert-icon">!</span>
              <div>
                <div className="mapping-alert-title">{copy.keyboard.windowsUnavailableTitle}</div>
                <div className="text-xs text-secondary">{copy.keyboard.windowsUnavailableDescription}</div>
              </div>
            </div>
            <button className="btn btn-primary" disabled type="button">
              {copy.keyboard.windowsUnavailableAction}
            </button>
          </div>
        ) : null}

        {shouldShowAccessibilityAlert ? (
          <div className="mapping-alert">
            <div className="mapping-alert-copy">
              <span className="mapping-alert-icon">⚠</span>
              <div>
                <div className="mapping-alert-title">{copy.keyboard.permissionTitle}</div>
                <div className="text-xs text-secondary">{copy.keyboard.permissionDescription}</div>
              </div>
            </div>
            <button className="btn btn-primary" onClick={openAccessibilitySettings} type="button">
              {copy.keyboard.permissionAction}
            </button>
          </div>
        ) : null}

        <div className="mapping-summary-grid">
          <div className="mapping-summary-card">
            <div className="mapping-summary-card-header">
              <div>
                <span className="label">{copy.keyboard.selectedLayer}</span>
                <h3>{activeLayer.title}</h3>
              </div>
              <div className="mapping-summary-toggle">
                <span className="text-xs text-secondary">
                  {status.enabled ? copy.keyboard.enabled : copy.keyboard.paused}
                </span>
                <MiniToggle
                  active={status.enabled}
                  onChange={setEnabled}
                  ariaLabel={language === 'zh' ? '切换映射模式' : 'Toggle mapping mode'}
                  disabled={!status.running}
                />
              </div>
            </div>
            <p className="text-sm text-secondary">{activeLayer.description}</p>
          </div>
        </div>

        <div className="layer-switcher">
          {layers.map((layer) => {
            return (
              <div
                key={layer.id}
                className={[
                  'layer-switcher-item',
                  layer.id,
                  layer.id === activeLayer.id ? 'active' : '',
                ].join(' ')}
              >
                <button
                  type="button"
                  className="layer-switcher-pill-toggle"
                  onClick={() => setActiveLayerId(layer.id)}
                >
                  <span className="layer-switcher-pill-label">{layer.label}</span>
                </button>
              </div>
            );
          })}
        </div>

        <div className={`keyboard-stage ${status.enabled ? '' : 'layer-disabled'}`}>
          <div className={`keyboard-shell ${activeLayer.id}`}>
            <div className="keyboard-stage-header">
              <div>
                <div className="label">{copy.keyboard.liveLayer}</div>
                <div className="keyboard-stage-title">{activeLayer.label}</div>
              </div>
              <div className="keyboard-stage-modifier">
                <span className="keycap modifier">{activeLayer.modifierKey}</span>
                <span className="text-xs text-secondary">{copy.keyboard.activeLayerHint}</span>
              </div>
            </div>

            <div className="keyboard-viewport">
              <div className="keyboard-surface">
                {keyboardRows.map((row, rowIndex) => (
                  <div key={rowIndex} className="keyboard-row">
                    {row.map((keyItem) => (
                      <KeyboardKey
                        key={keyItem.id}
                        keyItem={keyItem}
                        layer={activeLayer}
                        holdLabel={copy.keyboard.hold}
                        modeTag={copy.keyboard.modeTag}
                      />
                    ))}
                  </div>
                ))}
              </div>
            </div>

            {activeLayer.combos.length ? (
              <div className="keyboard-combo-strip">
                <div className="label">{copy.keyboard.tapCombos}</div>
                <div className="combo-list">
                  {activeLayer.combos.map((combo) => (
                    <div key={combo.id} className="combo-card">
                      <div className="combo-trigger">
                        <span className="keycap modifier">;</span>
                        <span className="combo-trigger-text">{combo.trigger}</span>
                      </div>
                      <div className="combo-output">{combo.output}</div>
                      <div className="text-xs text-secondary">{combo.description}</div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        </div>
      </section>

      {otherRules.length > 0 ? (
      <div className="mapping-bottom-grid">
        <section className="card">
          <div className="card-header other-mappings-header">
            <div className="card-header-copy">
              <span className="card-title">{copy.keyboard.otherMappings}</span>
              <span className="label">{copy.keyboard.rulesLabel(otherRules.length)}</span>
            </div>
            <div className="other-mappings-master-toggle">
              <span className="text-xs text-secondary">
                {areOtherMappingsEnabled ? copy.keyboard.otherMappingsEnabled : copy.keyboard.otherMappingsPaused}
              </span>
              <MiniToggle
                active={areOtherMappingsEnabled}
                onChange={handleToggleOtherMappings}
                ariaLabel={copy.keyboard.toggleOtherMappings}
              />
            </div>
          </div>
          <div className="other-mappings-list">
            {otherRules.map((rule) => {
              const localizedRule = getLocalizedRuleTexts(rule, language);
              const ruleDisabled = disabledRules.includes(rule.id);

              return (
                <div key={rule.id} className={`rule-row ${ruleDisabled ? 'rule-disabled' : ''}`}>
                  <div className="rule-from flex items-center gap-2">
                    {rule.modifierDisplay ? <span className="keycap modifier">{rule.modifierDisplay}</span> : null}
                    <span className="keycap">{rule.fromDisplay}</span>
                  </div>
                  <span className="rule-arrow">→</span>
                  <div className="rule-to flex items-center gap-2">
                    <span className="keycap output">{localizedRule.toDisplay}</span>
                    <span className="text-sm text-secondary">{localizedRule.to}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      </div>
      ) : null}
    </div>
  );
}
