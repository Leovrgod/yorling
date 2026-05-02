import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Toggle } from '../common/Toggle';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';

type ClipboardItemKind = 'text' | 'image' | 'file' | 'group' | 'unknown';

interface ClipboardFileReference {
  path: string;
  name: string;
  type_label: string;
  byte_count: number | null;
  is_directory: boolean;
  is_image: boolean;
  image_data_url: string | null;
  width: number | null;
  height: number | null;
}

interface ClipboardHistoryItem {
  id: string;
  kind: ClipboardItemKind;
  preview_text: string | null;
  image_data_url: string | null;
  files: ClipboardFileReference[];
  item_count: number;
  byte_count: number;
  char_count: number | null;
  line_count: number | null;
  width: number | null;
  height: number | null;
  created_at: number;
  is_pinned: boolean;
  source_label: string | null;
  source_path: string | null;
  hash: string;
}

const NOTE_TONES = 6;
const NOTE_SHAPES = 5;
const MAX_VISIBLE_CLIPBOARD_ITEMS = 100;
const LONG_REORDER_DISTANCE = 720;

type NoteStyle = CSSProperties & Record<'--note-tilt', string>;
type ClipboardContextMenu = {
  itemId: string;
  x: number;
  y: number;
  hasLocalPath: boolean;
  isPinned: boolean;
};

function stableNumber(input: string): number {
  let hash = 2166136261;
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function getNoteTone(item: ClipboardHistoryItem): number {
  return stableNumber(item.hash || item.id) % NOTE_TONES;
}

function getNoteShape(item: ClipboardHistoryItem): number {
  return stableNumber(`${item.id}:${item.byte_count}`) % NOTE_SHAPES;
}

function getNoteTilt(item: ClipboardHistoryItem): string {
  const tilt = (stableNumber(item.id) % 7) - 3;
  return `${tilt * 0.6}deg`;
}

function formatTime(timestamp: number, language: 'zh' | 'en', now: number): string {
  const deltaMs = Math.max(0, now - timestamp);
  const minutes = Math.floor(deltaMs / 60_000);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) return language === 'zh' ? `${days} 天前` : `${days}d ago`;
  if (hours > 0) return language === 'zh' ? `${hours} 小时前` : `${hours}h ago`;
  if (minutes > 0) return language === 'zh' ? `${minutes} 分钟前` : `${minutes}m ago`;
  return language === 'zh' ? '刚刚' : 'Just now';
}

function formatBytes(byteCount: number): string {
  if (byteCount >= 1_048_576) {
    return `${(byteCount / 1_048_576).toFixed(byteCount >= 10_485_760 ? 0 : 1)} MB`;
  }
  if (byteCount >= 1024) {
    return `${Math.round(byteCount / 1024)} KB`;
  }
  return `${byteCount} B`;
}

function itemsAreEqual(left: ClipboardHistoryItem, right: ClipboardHistoryItem): boolean {
  return left.id === right.id
    && left.kind === right.kind
    && left.preview_text === right.preview_text
    && left.image_data_url === right.image_data_url
    && filesAreEqual(left.files, right.files)
    && left.item_count === right.item_count
    && left.byte_count === right.byte_count
    && left.char_count === right.char_count
    && left.line_count === right.line_count
    && left.width === right.width
    && left.height === right.height
    && left.created_at === right.created_at
    && left.is_pinned === right.is_pinned
    && left.source_label === right.source_label
    && left.source_path === right.source_path
    && left.hash === right.hash;
}

function filesAreEqual(left: ClipboardFileReference[], right: ClipboardFileReference[]): boolean {
  if (left.length !== right.length) return false;
  return left.every((file, index) => {
    const other = right[index];
    return file.path === other.path
      && file.name === other.name
      && file.type_label === other.type_label
      && file.byte_count === other.byte_count
      && file.is_directory === other.is_directory
      && file.is_image === other.is_image
      && file.image_data_url === other.image_data_url
      && file.width === other.width
      && file.height === other.height;
  });
}

function mergeHistoryItems(
  previous: ClipboardHistoryItem[],
  next: ClipboardHistoryItem[],
): ClipboardHistoryItem[] {
  const limitedNext = next.slice(0, MAX_VISIBLE_CLIPBOARD_ITEMS);
  const previousById = new Map(previous.map((item) => [item.id, item]));
  const merged = limitedNext.map((item) => {
    const previousItem = previousById.get(item.id);
    return previousItem && itemsAreEqual(previousItem, item) ? previousItem : item;
  });

  if (merged.length === previous.length && merged.every((item, index) => item === previous[index])) {
    return previous;
  }

  return merged;
}

function promoteHistoryItem(previous: ClipboardHistoryItem[], id: string): ClipboardHistoryItem[] {
  const index = previous.findIndex((item) => item.id === id);
  if (index <= 0) return previous;

  const next = previous.slice();
  const [item] = next.splice(index, 1);
  insertByPinnedState(next, { ...item, created_at: Date.now() });
  return next;
}

function removeHistoryItem(previous: ClipboardHistoryItem[], id: string): ClipboardHistoryItem[] {
  return previous.filter((item) => item.id !== id);
}

function updatePinnedState(previous: ClipboardHistoryItem[], id: string, isPinned: boolean): ClipboardHistoryItem[] {
  const index = previous.findIndex((item) => item.id === id);
  if (index < 0) return previous;

  const next = previous.slice();
  const [item] = next.splice(index, 1);
  insertByPinnedState(next, { ...item, is_pinned: isPinned });
  return next;
}

function insertByPinnedState(items: ClipboardHistoryItem[], item: ClipboardHistoryItem): void {
  if (item.is_pinned) {
    items.unshift(item);
    return;
  }

  const firstUnpinnedIndex = items.findIndex((candidate) => !candidate.is_pinned);
  items.splice(firstUnpinnedIndex < 0 ? items.length : firstUnpinnedIndex, 0, item);
}

function itemMatchesSearch(item: ClipboardHistoryItem, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;

  const haystack = [
    item.preview_text,
    item.source_label,
    item.source_path,
    item.kind,
    ...item.files.flatMap((file) => [
      file.name,
      file.path,
      file.type_label,
      file.is_directory ? 'folder' : 'file',
      file.is_image ? 'image' : null,
    ]),
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  return haystack.includes(normalized);
}

function primaryFile(item: ClipboardHistoryItem): ClipboardFileReference | null {
  return item.files[0] ?? null;
}

function isFileLikeItem(item: ClipboardHistoryItem): boolean {
  return item.kind === 'file' || item.files.length > 0;
}

function isImageGroup(item: ClipboardHistoryItem): boolean {
  return item.kind === 'group' && item.files.length > 0 && item.files.every((file) => file.is_image);
}

function fileSizeLabel(file: ClipboardFileReference): string {
  if (file.is_directory) return file.type_label;
  return file.byte_count === null ? file.type_label : `${file.type_label} · ${formatBytes(file.byte_count)}`;
}

function ClipboardNoteComponent({
  item,
  isCopied,
  now,
  onCopy,
  onDelete,
  onTogglePin,
  onContextMenu,
  registerNoteRef,
}: {
  item: ClipboardHistoryItem;
  isCopied: boolean;
  now: number;
  onCopy: (id: string) => void;
  onDelete: (id: string) => void;
  onTogglePin: (id: string, isPinned: boolean) => void;
  onContextMenu: (item: ClipboardHistoryItem, event: ReactMouseEvent<HTMLElement>) => void;
  registerNoteRef: (id: string, node: HTMLElement | null) => void;
}) {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language).clipboard;
  const tone = getNoteTone(item);
  const shape = getNoteShape(item);
  const tilt = getNoteTilt(item);
  const isImage = item.kind === 'image';
  const isFileLike = isFileLikeItem(item);
  const firstFile = primaryFile(item);
  const showsImagePreview = (isImage || item.kind === 'group') && !isFileLike && Boolean(item.image_data_url);

  const badge = item.kind === 'text'
    ? copy.textBadge
    : isImage
      ? copy.imageBadge
      : item.kind === 'file'
        ? (firstFile?.is_directory ? copy.folderBadge : copy.fileBadge)
        : isImageGroup(item) || (item.kind === 'group' && item.files.length === 0 && item.image_data_url)
          ? copy.imageGroupBadge
          : item.kind === 'group'
            ? copy.fileGroupBadge
            : copy.unsupportedBadge;

  const meta = useMemo(() => {
    if (item.kind === 'group') {
      return copy.itemsCount(item.item_count || item.files.length || 1);
    }

    if (item.kind === 'file' && firstFile) {
      return fileSizeLabel(firstFile);
    }

    if (isImage && item.width && item.height) {
      return copy.imageSize(item.width, item.height);
    }

    if (item.line_count && item.line_count > 1) {
      return copy.lines(item.line_count);
    }

    if (item.char_count !== null) {
      return copy.chars(item.char_count);
    }

    return formatBytes(item.byte_count);
  }, [copy, firstFile, isImage, item.byte_count, item.char_count, item.files.length, item.height, item.item_count, item.kind, item.line_count, item.width]);

  return (
    <article
      className={`clipboard-note clipboard-note--${item.kind} clipboard-note--tone-${tone} clipboard-note--shape-${shape}${item.is_pinned ? ' is-pinned' : ''}`}
      style={{ '--note-tilt': tilt } as NoteStyle}
      ref={(node) => registerNoteRef(item.id, node)}
      onContextMenu={(event) => onContextMenu(item, event)}
    >
      <button
        type="button"
        className="clipboard-note__surface"
        onClick={() => onCopy(item.id)}
        title={copy.copyItem}
      >
        <span className="clipboard-note__tape" aria-hidden="true" />
        <span className="clipboard-note__topline">
          <span className="clipboard-note__badge">{item.is_pinned ? copy.pinnedBadge : badge}</span>
          <span className="clipboard-note__time">{formatTime(item.created_at, language, now)}</span>
        </span>

        {showsImagePreview ? (
          <span className="clipboard-note__image-wrap">
            {item.image_data_url ? (
              <img src={item.image_data_url} alt="" className="clipboard-note__image" />
            ) : (
              <span className="clipboard-note__image-fallback" aria-hidden="true" />
            )}
          </span>
        ) : isFileLike ? (
          <FilePreview item={item} />
        ) : (
          <span className="clipboard-note__text">
            {item.preview_text}
          </span>
        )}

        <span className="clipboard-note__footer">
          <span>{meta}</span>
          <span>{isCopied ? copy.copied : item.source_label ?? formatBytes(item.byte_count)}</span>
        </span>
      </button>
      <span className="clipboard-note__quick-actions">
        <button
          type="button"
          className="clipboard-note__quick-action"
          onClick={() => onTogglePin(item.id, !item.is_pinned)}
          title={item.is_pinned ? copy.unpinItem : copy.pinItem}
          aria-label={item.is_pinned ? copy.unpinItem : copy.pinItem}
        >
          {item.is_pinned ? '↓' : '↑'}
        </button>
        <button
          type="button"
          className="clipboard-note__quick-action clipboard-note__quick-action--danger"
          onClick={() => onDelete(item.id)}
          title={copy.deleteItem}
          aria-label={copy.deleteItem}
        >
          ×
        </button>
      </span>
    </article>
  );
}

const ClipboardNote = memo(ClipboardNoteComponent);

function FilePreview({ item }: { item: ClipboardHistoryItem }) {
  const files = item.files.length > 0 ? item.files : [];
  const firstFile = files[0];

  if (item.kind === 'group') {
    return (
      <span className="clipboard-note__file-grid" aria-hidden="true">
        {files.slice(0, 4).map((file) => (
          <span key={file.path} className="clipboard-note__file-tile">
            {file.image_data_url ? (
              <img src={file.image_data_url} alt="" className="clipboard-note__file-thumb" />
            ) : (
              <span className="clipboard-note__file-mark">{file.type_label.slice(0, 4)}</span>
            )}
          </span>
        ))}
      </span>
    );
  }

  return (
    <span className="clipboard-note__file">
      {firstFile?.image_data_url ? (
        <span className="clipboard-note__image-wrap">
          <img src={firstFile.image_data_url} alt="" className="clipboard-note__image" />
        </span>
      ) : (
        <span className="clipboard-note__file-icon" aria-hidden="true">
          {firstFile?.type_label.slice(0, 4) ?? 'FILE'}
        </span>
      )}
      <span className="clipboard-note__file-name">{firstFile?.name ?? item.preview_text}</span>
      <span className="clipboard-note__file-path">{firstFile?.path ?? item.source_path}</span>
    </span>
  );
}

export function ClipboardWall() {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language).clipboard;
  const [items, setItems] = useState<ClipboardHistoryItem[]>([]);
  const [enabled, setEnabled] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [now, setNow] = useState(() => Date.now());
  const [contextMenu, setContextMenu] = useState<ClipboardContextMenu | null>(null);
  const copiedTimerRef = useRef<number | null>(null);
  const noteRefs = useRef(new Map<string, HTMLElement>());
  const previousRectsRef = useRef(new Map<string, DOMRect>());

  const visibleItems = useMemo(
    () => items.filter((item) => itemMatchesSearch(item, searchQuery)),
    [items, searchQuery],
  );

  const applyItems = useCallback((nextItems: ClipboardHistoryItem[]) => {
    setItems((previous) => mergeHistoryItems(previous, nextItems));
  }, []);

  const fetchHistory = useCallback(async () => {
    try {
      const [nextItems, nextEnabled] = await Promise.all([
        invoke<ClipboardHistoryItem[]>('get_clipboard_history'),
        invoke<boolean>('get_clipboard_monitor_enabled'),
      ]);
      applyItems(nextItems);
      setEnabled(nextEnabled);
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [applyItems]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    void fetchHistory();
    const historyInterval = window.setInterval(() => {
      void fetchHistory();
    }, 15000);
    const clockInterval = window.setInterval(() => setNow(Date.now()), 60000);

    void listen('clipboard-history-updated', () => {
      void fetchHistory();
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
    }).catch(() => {
      // Older dev shells may not expose Tauri events; the slow refresh keeps the view usable.
    });

    return () => {
      unlisten?.();
      window.clearInterval(historyInterval);
      window.clearInterval(clockInterval);
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current);
      }
    };
  }, [fetchHistory]);

  useEffect(() => {
    if (!contextMenu) return undefined;

    const closeMenu = () => setContextMenu(null);
    window.addEventListener('click', closeMenu);
    window.addEventListener('keydown', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);

    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('keydown', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
    };
  }, [contextMenu]);

  useLayoutEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const nextRects = new Map<string, DOMRect>();

    noteRefs.current.forEach((node, id) => {
      const nextRect = node.getBoundingClientRect();
      const previousRect = previousRectsRef.current.get(id);
      nextRects.set(id, nextRect);

      if (reducedMotion || !node.animate) return;

      const finalTransform = window.getComputedStyle(node).transform;
      const settledTransform = finalTransform === 'none' ? '' : finalTransform;

      if (!previousRect) {
        node.animate(
          [
            { opacity: 0, transform: `translateY(-8px) scale(0.97) ${settledTransform}` },
            { opacity: 1, transform: settledTransform },
          ],
          { duration: 260, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' },
        );
        return;
      }

      const dx = previousRect.left - nextRect.left;
      const dy = previousRect.top - nextRect.top;
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;

      const distance = Math.hypot(dx, dy);
      const firstFrame = distance > LONG_REORDER_DISTANCE
        ? { opacity: 0.18, transform: `translateY(10px) scale(0.98) ${settledTransform}` }
        : { opacity: 1, transform: `translate(${dx}px, ${dy}px) ${settledTransform}` };

      node.animate(
        [
          firstFrame,
          { opacity: 1, transform: settledTransform },
        ],
        { duration: distance > LONG_REORDER_DISTANCE ? 220 : 320, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' },
      );
    });

    previousRectsRef.current = nextRects;
  }, [visibleItems]);

  const registerNoteRef = useCallback((id: string, node: HTMLElement | null) => {
    if (node) {
      noteRefs.current.set(id, node);
      return;
    }
    noteRefs.current.delete(id);
  }, []);

  const handleToggle = async (nextEnabled: boolean) => {
    setEnabled(nextEnabled);
    try {
      await invoke('set_clipboard_monitor_enabled', { enabled: nextEnabled });
      await fetchHistory();
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(message);
      setEnabled(!nextEnabled);
    }
  };

  const handleClear = async () => {
    setBusy(true);
    try {
      await invoke('clear_clipboard_history');
      applyItems([]);
      setCopiedId(null);
      previousRectsRef.current = new Map();
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(message);
    } finally {
      setBusy(false);
    }
  };

  const handleCopyItem = useCallback(async (id: string) => {
    setItems((previous) => promoteHistoryItem(previous, id));
    try {
      await invoke('copy_clipboard_history_item', { id });
      setCopiedId(id);
      setNow(Date.now());
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current);
      }
      copiedTimerRef.current = window.setTimeout(() => setCopiedId(null), 1400);
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(`${copy.copyErrorPrefix}: ${message}`);
      void fetchHistory();
    }
  }, [copy.copyErrorPrefix, fetchHistory]);

  const handleDeleteItem = useCallback(async (id: string) => {
    setItems((previous) => removeHistoryItem(previous, id));
    setContextMenu(null);
    try {
      await invoke('delete_clipboard_history_item', { id });
      setCopiedId((previous) => (previous === id ? null : previous));
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(`${copy.deleteErrorPrefix}: ${message}`);
      void fetchHistory();
    }
  }, [copy.deleteErrorPrefix, fetchHistory]);

  const handleTogglePin = useCallback(async (id: string, isPinned: boolean) => {
    setItems((previous) => updatePinnedState(previous, id, isPinned));
    setContextMenu((previous) => previous && previous.itemId === id ? { ...previous, isPinned } : previous);
    try {
      await invoke('set_clipboard_history_item_pinned', { id, pinned: isPinned });
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(`${copy.pinErrorPrefix}: ${message}`);
      void fetchHistory();
    }
  }, [copy.pinErrorPrefix, fetchHistory]);

  const handleNoteContextMenu = useCallback((
    item: ClipboardHistoryItem,
    event: ReactMouseEvent<HTMLElement>,
  ) => {
    event.preventDefault();
    setContextMenu({
      itemId: item.id,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 222)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 132)),
      hasLocalPath: Boolean(item.source_path),
      isPinned: item.is_pinned,
    });
  }, []);

  const handleOpenItemLocation = useCallback(async (id: string) => {
    setContextMenu(null);
    try {
      await invoke('open_clipboard_item_location', { id });
      setError(null);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(`${copy.openLocationErrorPrefix}: ${message}`);
    }
  }, [copy.openLocationErrorPrefix]);

  return (
    <div className="clipboard-page page-enter">
      <header className="clipboard-heading">
        <div>
          <h1>{copy.title}</h1>
          <p>{copy.subtitle}</p>
        </div>
        <div className="clipboard-heading__actions">
          <Toggle
            active={enabled}
            onChange={handleToggle}
            label={enabled ? copy.enabled : copy.disabled}
          />
          <button type="button" className="clipboard-action" onClick={() => void fetchHistory()}>
            {copy.refresh}
          </button>
          <button
            type="button"
            className="clipboard-action clipboard-action--quiet"
            onClick={() => void handleClear()}
            disabled={busy || items.length === 0}
          >
            {copy.clear}
          </button>
        </div>
      </header>

      <label className="clipboard-search">
        <span className="clipboard-search__icon" aria-hidden="true">⌕</span>
        <input
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
          placeholder={copy.searchPlaceholder}
          aria-label={copy.searchPlaceholder}
        />
      </label>

      {error ? <div className="clipboard-error">{error}</div> : null}

      {items.length === 0 ? (
        <div className={`clipboard-empty ${loading ? 'is-loading' : ''}`}>
          <div className="clipboard-empty__mark" aria-hidden="true">▥</div>
          <h2>{copy.emptyTitle}</h2>
          <p>{copy.emptyDescription}</p>
        </div>
      ) : visibleItems.length === 0 ? (
        <div className="clipboard-empty clipboard-empty--compact">
          <div className="clipboard-empty__mark" aria-hidden="true">⌕</div>
          <h2>{copy.searchNoResults}</h2>
        </div>
      ) : (
        <section className="clipboard-note-wall" aria-label={copy.title}>
          {visibleItems.map((item) => (
            <ClipboardNote
              key={item.id}
              item={item}
              isCopied={copiedId === item.id}
              now={now}
              onCopy={handleCopyItem}
              onDelete={handleDeleteItem}
              onTogglePin={handleTogglePin}
              onContextMenu={handleNoteContextMenu}
              registerNoteRef={registerNoteRef}
            />
          ))}
        </section>
      )}

      {contextMenu ? (
        <div
          className="clipboard-context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          role="menu"
        >
          <button
            type="button"
            role="menuitem"
            className="clipboard-context-menu__item"
            disabled={!contextMenu.hasLocalPath}
            onClick={() => void handleOpenItemLocation(contextMenu.itemId)}
          >
            {contextMenu.hasLocalPath ? copy.openFileLocation : copy.fileLocationUnavailable}
          </button>
          <button
            type="button"
            role="menuitem"
            className="clipboard-context-menu__item"
            onClick={() => void handleTogglePin(contextMenu.itemId, !contextMenu.isPinned)}
          >
            {contextMenu.isPinned ? copy.unpinItem : copy.pinItem}
          </button>
          <button
            type="button"
            role="menuitem"
            className="clipboard-context-menu__item clipboard-context-menu__item--danger"
            onClick={() => void handleDeleteItem(contextMenu.itemId)}
          >
            {copy.deleteItem}
          </button>
        </div>
      ) : null}
    </div>
  );
}
