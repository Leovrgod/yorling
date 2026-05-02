import type { IslandSession, IslandToolHistoryItem } from '../types';

export const IDLE_SESSION_VISIBILITY_TIMEOUT_MS = 20 * 60 * 1000;
export const COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS = 10 * 60 * 1000;

export function sortIslandSessionsForList(sessions: IslandSession[]): IslandSession[] {
  const groupedSessions = new Map<string, IslandSession[]>();

  for (const session of sessions) {
    const group = groupedSessions.get(session.provider_id);
    if (group) {
      group.push(session);
      continue;
    }

    groupedSessions.set(session.provider_id, [session]);
  }

  return [...groupedSessions.entries()]
    .map(([providerId, providerSessions]) => ({
      providerId,
      sessions: [...providerSessions].sort(compareSessionsForList),
    }))
    .sort((left, right) => {
      const leftRepresentative = left.sessions[left.sessions.length - 1] ?? null;
      const rightRepresentative = right.sessions[right.sessions.length - 1] ?? null;

      if (leftRepresentative && rightRepresentative) {
        const representativeOrder = compareSessionsForList(leftRepresentative, rightRepresentative);
        if (representativeOrder !== 0) {
          return representativeOrder;
        }
      }

      return left.providerId.localeCompare(right.providerId);
    })
    .flatMap((group) => group.sessions);
}

export function filterVisibleIslandSessions(
  sessions: IslandSession[],
  now = Date.now(),
): IslandSession[] {
  return sessions.filter((session) => isIslandSessionVisible(session, now));
}

export function filterCollapsedIslandSessions(
  sessions: IslandSession[],
  now = Date.now(),
): IslandSession[] {
  return sessions.filter((session) => isCollapsedIslandSessionVisible(session, now));
}

export function isIslandSessionVisible(
  session: IslandSession,
  now = Date.now(),
): boolean {
  return isIdleSessionVisibleWithin(session, IDLE_SESSION_VISIBILITY_TIMEOUT_MS, now);
}

export function isCollapsedIslandSessionVisible(
  session: IslandSession,
  now = Date.now(),
): boolean {
  return isIdleSessionVisibleWithin(session, COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS, now);
}

export function getPrimaryIslandSession(sessions: IslandSession[]): IslandSession | null {
  if (sessions.length === 0) {
    return null;
  }

  const attentionSessions = sessions.filter(isAttentionSession);
  if (attentionSessions.length > 0) {
    return attentionSessions.reduce((latest, session) =>
      compareSessionsByAttention(session, latest) > 0 ? session : latest,
    );
  }

  const processingSessions = sessions.filter((session) => session.phase === 'processing');
  if (processingSessions.length > 0) {
    return processingSessions.reduce((latest, session) =>
      compareSessionsByActivity(session, latest) > 0 ? session : latest,
    );
  }

  return sessions.reduce((latest, session) =>
    compareSessionsByActivity(session, latest) > 0 ? session : latest,
  );
}

export function getLatestAttentionSessionId(sessions: IslandSession[]): string | null {
  const primarySession = getPrimaryIslandSession(sessions);
  return primarySession?.id ?? null;
}

function compareSessionsByAttention(left: IslandSession, right: IslandSession) {
  const leftAttentionTs = getSessionAttentionTimestamp(left) ?? getSessionActivityTimestamp(left);
  const rightAttentionTs = getSessionAttentionTimestamp(right) ?? getSessionActivityTimestamp(right);

  if (leftAttentionTs !== rightAttentionTs) {
    return leftAttentionTs - rightAttentionTs;
  }

  return compareSessionsByActivity(left, right);
}

function compareSessionsByActivity(left: IslandSession, right: IslandSession) {
  const leftActivityTs = getSessionActivityTimestamp(left);
  const rightActivityTs = getSessionActivityTimestamp(right);

  if (leftActivityTs !== rightActivityTs) {
    return leftActivityTs - rightActivityTs;
  }

  return left.id.localeCompare(right.id);
}

function compareSessionsForList(left: IslandSession, right: IslandSession) {
  const leftNeedsAttention = isAttentionSession(left);
  const rightNeedsAttention = isAttentionSession(right);

  if (leftNeedsAttention !== rightNeedsAttention) {
    return leftNeedsAttention ? 1 : -1;
  }

  if (leftNeedsAttention && rightNeedsAttention) {
    const leftAttentionTs = getSessionAttentionTimestamp(left) ?? getSessionActivityTimestamp(left);
    const rightAttentionTs = getSessionAttentionTimestamp(right) ?? getSessionActivityTimestamp(right);

    if (leftAttentionTs !== rightAttentionTs) {
      return leftAttentionTs - rightAttentionTs;
    }
  }

  return compareSessionsByActivity(left, right);
}

export function isAttentionSession(session: IslandSession) {
  return (
    session.phase === 'waitingforapproval' ||
    session.phase === 'waitingforanswer' ||
    session.pending_permission !== null ||
    session.pending_question !== null
  );
}

function getSessionAttentionTimestamp(session: IslandSession) {
  const latestAttention = Math.max(
    session.pending_permission?.received_at ?? Number.NEGATIVE_INFINITY,
    session.pending_question?.received_at ?? Number.NEGATIVE_INFINITY,
  );

  return Number.isFinite(latestAttention) ? latestAttention : null;
}

export function getSessionActivityTimestamp(session: IslandSession) {
  return Math.max(
    session.started_at,
    session.ended_at ?? Number.NEGATIVE_INFINITY,
    getLatestChatTimestamp(session),
    getLatestToolTimestamp(session.transcript_preview?.tool_history ?? []),
    getSessionAttentionTimestamp(session) ?? Number.NEGATIVE_INFINITY,
  );
}

function isIdleSessionVisibleWithin(
  session: IslandSession,
  timeoutMs: number,
  now: number,
) {
  if (session.phase !== 'idle' && session.phase !== 'ended') {
    return true;
  }

  return now - getSessionActivityTimestamp(session) <= timeoutMs;
}

function getLatestChatTimestamp(session: IslandSession) {
  const chatMessages = [
    ...session.chat_messages,
    ...(session.transcript_preview?.chat_preview ?? []),
  ];
  return chatMessages.reduce(
    (latest, message) => Math.max(latest, message.timestamp),
    Number.NEGATIVE_INFINITY,
  );
}

function getLatestToolTimestamp(toolHistory: IslandToolHistoryItem[]) {
  return toolHistory.reduce(
    (latest, item) => Math.max(latest, item.finished_at ?? item.started_at ?? Number.NEGATIVE_INFINITY),
    Number.NEGATIVE_INFINITY,
  );
}
