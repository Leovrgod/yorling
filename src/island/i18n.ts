import { getStoredLanguage } from '../i18n/copy.ts';

type IslandLang = 'en' | 'zh';

const translations: Record<string, Record<IslandLang, string>> = {
  // Phase labels
  'phase.idle': { en: 'Idle', zh: '空闲' },
  'phase.processing': { en: 'Working', zh: '运行中' },
  'phase.waitingforapproval': { en: 'Needs Approval', zh: '待审批' },
  'phase.waitingforanswer': { en: 'Asking', zh: '提问中' },
  'phase.compacting': { en: 'Compacting', zh: '压缩中' },
  'phase.ended': { en: 'Done', zh: '完成' },

  // Chip labels
  'chip.idle': { en: 'Idle', zh: '空闲' },
  'chip.live': { en: 'Live', zh: '运行' },
  'chip.action': { en: 'Action', zh: '待办' },
  'chip.trim': { en: 'Trim', zh: '整理' },

  // Collapsed bar
  'collapsed.dynamic_island': { en: 'Dynamic Island', zh: '灵动岛' },
  'collapsed.requests_waiting': { en: 'request(s) waiting', zh: '个请求等待' },
  'collapsed.approval_needed': { en: 'Needs approval', zh: '等待审批' },
  'collapsed.question_waiting': { en: 'Question waiting', zh: '等待回答' },
  'collapsed.yorling': { en: 'Yorling', zh: 'Yorling' },

  // Session list
  'sessions.title': { en: 'Live Sessions', zh: '活跃会话' },
  'sessions.no_live': { en: 'No live sessions', zh: '暂无活跃会话' },
  'sessions.live_count': { en: '{n} live session{suffix}', zh: '{n} 个活跃会话' },
  'sessions.no_active': { en: 'No active sessions', zh: '暂无活跃会话' },
  'sessions.rules': { en: 'Approval rules', zh: '审批规则' },
  'sessions.collapse': { en: 'Collapse island', zh: '收起灵动岛' },

  // Session card
  'session.awaiting_approval': { en: 'Awaiting approval for {tool}', zh: '等待审批 {tool}' },
  'session.subagents': { en: 'subagent(s)', zh: '个子代理' },
  'session.waiting': { en: 'Waiting for the next event', zh: '等待下一事件' },
  'session.jump': { en: 'Jump to terminal', zh: '跳转到终端' },

  // Permission card
  'permission.deny': { en: 'Deny', zh: '拒绝' },
  'permission.allow': { en: 'Allow', zh: '允许' },
  'permission.always': { en: 'Always', zh: '总是' },
  'permission.expired': { en: 'This approval request has expired — the agent is no longer waiting.', zh: '此审批请求已过期，代理已不再等待响应。' },
  'tool.unknown': { en: 'Action', zh: '操作' },

  // Question card
  'question.placeholder': { en: 'Type a custom answer...', zh: '输入自定义回答...' },
  'question.send': { en: 'Send', zh: '发送' },

  // ChatView
  'chatview.back': { en: 'Back to session list', zh: '返回会话列表' },
  'chatview.tool_calls': { en: 'Tool Calls', zh: '工具调用' },
  'chatview.session_info': { en: 'Session Info', zh: '会话信息' },
  'chatview.session': { en: 'Session', zh: '会话' },
  'chatview.started': { en: 'Started', zh: '开始' },
  'chatview.ended': { en: 'Ended', zh: '结束' },
  'chatview.subagents': { en: 'Subagents', zh: '子代理' },
  'chatview.working_dir': { en: 'Working Dir', zh: '工作目录' },
  'chatview.terminal': { en: 'Terminal', zh: '终端' },
  'chatview.turns': { en: 'Turns', zh: '轮数' },
  'chatview.input': { en: 'Input', zh: '输入' },
  'chatview.output': { en: 'Output', zh: '输出' },
  'chatview.prompts': { en: 'Your prompts', zh: '你的提示词' },
  'chatview.decisions': { en: 'Needs your choice', zh: '需要你选择' },
  'chatview.empty': { en: 'No prompts yet', zh: '还没有提示词' },
  'chatview.copy': { en: 'Copy', zh: '复制' },
  'chatview.copied': { en: 'Copied', zh: '已复制' },
  'chatview.new_message': { en: '1 new message', zh: '1 条新消息' },
  'chatview.new_messages': { en: '{n} new messages', zh: '{n} 条新消息' },

  // Approval rules
  'rules.title': { en: 'Approval Rules', zh: '审批规则' },
  'rules.clear_all': { en: 'Clear All', zh: '清除全部' },
  'rules.loading': { en: 'Loading...', zh: '加载中...' },
  'rules.empty': { en: 'No saved rules. Use "Always" when approving a permission to create one.', zh: '暂无规则。审批权限时选择"总是"即可创建规则。' },
  'rules.allow': { en: 'Allow', zh: '允许' },
  'rules.deny': { en: 'Deny', zh: '拒绝' },

  // Transcript
  'transcript.synced': { en: 'Synced', zh: '已同步' },
  'transcript.degraded': { en: 'Degraded', zh: '降级' },
  'transcript.unavailable': { en: 'No transcript', zh: '无记录' },
  'transcript.just_now': { en: 'just now', zh: '刚才' },
  'time.minute': { en: '{n} min', zh: '{n} 分钟' },
  'time.hour': { en: '{n} hr', zh: '{n} 小时' },
  'time.day': { en: '{n} d', zh: '{n} 天' },

  // Roles
  'role.ai': { en: 'AI', zh: 'AI' },
  'role.you': { en: 'You', zh: '你' },
  'role.sys': { en: 'Sys', zh: '系统' },

  // Providers
  'provider.claude-code': { en: 'Claude Code', zh: 'Claude Code' },
  'provider.codex': { en: 'Codex CLI', zh: 'Codex CLI' },
  'provider.gemini': { en: 'Gemini CLI', zh: 'Gemini CLI' },
  'provider.cursor': { en: 'Cursor', zh: 'Cursor' },
  'provider.copilot': { en: 'GitHub Copilot', zh: 'GitHub Copilot' },
  'provider.hermes': { en: 'Hermes', zh: 'Hermes' },
  'provider.openclaw': { en: 'OpenClaw', zh: 'OpenClaw' },
  'provider.opencode': { en: 'OpenCode', zh: 'OpenCode' },
  'provider.qwen-code': { en: 'Qwen Code', zh: 'Qwen Code' },
  'provider.qoder': { en: 'Qoder', zh: 'Qoder' },
  'provider.qoderwork': { en: 'QoderWork', zh: 'QoderWork' },
  'provider.codebuddy': { en: 'CodeBuddy', zh: 'CodeBuddy' },
  'provider.workbuddy': { en: 'WorkBuddy', zh: 'WorkBuddy' },
};

let currentLang: IslandLang = 'en';

/**
 * Detect system language and set accordingly.
 */
export function initIslandI18n() {
  currentLang = resolveIslandLang(
    typeof window !== 'undefined' ? window.localStorage : undefined,
  );
}

/**
 * Get the current language.
 */
export function getIslandLang(): IslandLang {
  return currentLang;
}

/**
 * Set the language explicitly.
 */
export function setIslandLang(lang: IslandLang) {
  currentLang = lang;
}

export function syncIslandLangFromStorage(storage?: Pick<Storage, 'getItem'> | null): IslandLang {
  currentLang = resolveIslandLang(storage);
  return currentLang;
}

/**
 * Translate a key to the current language.
 * Supports simple {key} interpolation via optional params.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const entry = translations[key];
  if (!entry) return key;
  let result = entry[currentLang] ?? entry.en ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      result = result.replace(`{${k}}`, String(v));
    }
  }
  return result;
}

function resolveIslandLang(storage?: Pick<Storage, 'getItem'> | null): IslandLang {
  const storedLang = getStoredLanguage(storage);
  if (storedLang === 'zh' || storedLang === 'en') {
    return storedLang;
  }

  const sysLang = typeof navigator === 'undefined' ? 'en' : navigator.language || 'en';
  return sysLang.startsWith('zh') ? 'zh' : 'en';
}
