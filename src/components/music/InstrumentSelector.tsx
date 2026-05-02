import { useCallback } from 'react';
import {
  INSTRUMENTS,
  INSTRUMENT_CATEGORIES,
  getInstrumentDef,
  type InstrumentDef,
} from '../../data/instruments';
import { useAppStore } from '../../stores/appStore';
import type { AppLanguageId } from '../../types';

interface InstrumentSelectorProps {
  selectedId: string;
  recentInstrumentIds: string[];
  onSelect: (id: string) => void;
  onRemoveRecent: (id: string) => void;
  loading: boolean;
  error: string | null;
}

const CATEGORY_INSTRUMENTS = new Map<string, InstrumentDef[]>();
for (const inst of INSTRUMENTS) {
  const list = CATEGORY_INSTRUMENTS.get(inst.categoryId) ?? [];
  list.push(inst);
  CATEGORY_INSTRUMENTS.set(inst.categoryId, list);
}

export function InstrumentSelector({
  selectedId,
  recentInstrumentIds,
  onSelect,
  onRemoveRecent,
  loading,
  error,
}: InstrumentSelectorProps) {
  const language = useAppStore((s) => s.language) as AppLanguageId;

  const handleSelect = useCallback(
    (id: string) => {
      if (id !== selectedId) {
        onSelect(id);
      }
    },
    [selectedId, onSelect],
  );

  const selectedDef = getInstrumentDef(selectedId);
  const selectedLabel = selectedDef?.label[language] ?? selectedId;
  const recentInstrumentIdSet = new Set(recentInstrumentIds);
  const groupedRecentCategories = INSTRUMENT_CATEGORIES.map((category) => ({
    category,
    instruments: (CATEGORY_INSTRUMENTS.get(category.id) ?? []).filter((instrument) =>
      recentInstrumentIdSet.has(instrument.id),
    ),
  })).filter(({ instruments }) => instruments.length > 0);

  return (
    <div className="instrument-selector">
      <div className="instrument-selector-header">
        <span className="instrument-selector-title">
          {language === 'zh' ? '🎵 乐器' : '🎵 Instrument'}
        </span>
        <span className="instrument-selector-current">
          {selectedLabel}
          {loading && (
            <span className="instrument-loading-badge">
              {language === 'zh' ? '加载中…' : 'Loading…'}
            </span>
          )}
          {error && (
            <span className="instrument-error-badge" title={error}>
              {language === 'zh' ? '加载失败' : 'Load failed'}
            </span>
          )}
        </span>
      </div>

      <div className="instrument-selector-body">
        <div className="instrument-selector-main">
          <div className="instrument-categories">
            {INSTRUMENT_CATEGORIES.map((cat) => {
              const instruments = CATEGORY_INSTRUMENTS.get(cat.id);
              if (!instruments?.length) return null;

              return (
                <div key={cat.id} className="instrument-category">
                  <div className="instrument-category-label">
                    {cat.label[language]}
                  </div>
                  <div className="instrument-category-grid">
                    {instruments.map((inst) => {
                      const isSelected = inst.id === selectedId;
                      return (
                        <button
                          key={inst.id}
                          type="button"
                          className={`instrument-chip${isSelected ? ' selected' : ''}`}
                          onClick={() => handleSelect(inst.id)}
                          title={inst.label.en}
                        >
                          <span className="instrument-chip-label">
                            {inst.label[language]}
                          </span>
                          {inst.builtIn && (
                            <span className="instrument-builtin-badge">
                              {language === 'zh' ? '内置' : 'Built-in'}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <aside className="instrument-recent-panel">
          {groupedRecentCategories.length ? (
            <div className="instrument-recent-scroll">
              {groupedRecentCategories.map(({ category, instruments }) => (
                <div key={category.id} className="instrument-recent-group">
                  <div className="instrument-category-label">
                    {category.label[language]}
                  </div>

                  <div className="instrument-recent-grid">
                    {instruments.map((inst) => {
                      const isSelected = inst.id === selectedId;

                      return (
                        <div key={inst.id} className="instrument-recent-item">
                          <button
                            type="button"
                            className={`instrument-recent-button${isSelected ? ' selected' : ''}`}
                            onClick={() => handleSelect(inst.id)}
                            title={inst.label.en}
                          >
                            <span className="instrument-recent-label">{inst.label[language]}</span>
                            {inst.builtIn ? (
                              <span className="instrument-builtin-badge">
                                {language === 'zh' ? '内置' : 'Built-in'}
                              </span>
                            ) : null}
                          </button>

                          <button
                            type="button"
                            className="instrument-recent-remove"
                            onClick={() => onRemoveRecent(inst.id)}
                            title={language === 'zh' ? `移除 ${inst.label[language]}` : `Remove ${inst.label.en}`}
                            aria-label={language === 'zh'
                              ? `从最近使用中移除 ${inst.label[language]}`
                              : `Remove ${inst.label.en} from recent instruments`}
                          >
                            ×
                          </button>
                        </div>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="instrument-recent-empty">
              <span className="instrument-recent-empty-title">
                {language === 'zh' ? '还没有最近使用的音色' : 'No recent instruments yet'}
              </span>
              <span className="instrument-recent-empty-copy">
                {language === 'zh'
                  ? '切换过的音色会出现在这里，方便快速找回。'
                  : 'Instruments you switch to will appear here for quick access.'}
              </span>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
