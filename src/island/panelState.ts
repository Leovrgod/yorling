import type { IslandViewMode } from '../types';
import type { IslandOpenReason } from './hooks/useIslandAnimation';

interface ResolveExpandedPanelHeightInput {
  activeSurfaceScrollHeight: number;
  activeSurfaceOffsetHeight: number;
  activeInnerScrollHeight: number;
  previousMeasuredHeight: number | null;
}

export function shouldExpandForSessionSelection(viewMode: IslandViewMode) {
  return viewMode !== 'expanded';
}

export function shouldUseDomMouseLeave(
  viewMode: IslandViewMode,
  openReason: IslandOpenReason,
  usesBoundedNativeWindow = false,
) {
  return usesBoundedNativeWindow || viewMode !== 'expanded' || openReason !== 'hover';
}

export function shouldAutoCollapseExpandedIsland(hasAttentionRequest: boolean) {
  return !hasAttentionRequest;
}

export function promoteIslandOpenReasonForInternalInteraction(
  viewMode: IslandViewMode,
  openReason: IslandOpenReason,
) {
  if (viewMode === 'expanded' && openReason === 'hover') {
    return 'click';
  }

  return openReason;
}

export function resolveExpandedPanelHeight({
  activeSurfaceScrollHeight,
  activeSurfaceOffsetHeight,
  activeInnerScrollHeight,
  previousMeasuredHeight,
}: ResolveExpandedPanelHeightInput) {
  const candidates = [
    activeSurfaceScrollHeight,
    activeSurfaceOffsetHeight,
    activeInnerScrollHeight,
  ].filter((value) => Number.isFinite(value) && value > 0);

  if (candidates.length === 0) {
    return previousMeasuredHeight;
  }

  return Math.round(Math.max(...candidates));
}
