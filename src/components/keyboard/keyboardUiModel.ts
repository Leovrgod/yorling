import type { AppLanguageId, MappingRule } from '../../types';

export type KeyboardLayerId = 'space' | 'number' | 'symbol' | 'mouse';

export interface KeyboardLayoutKey {
  id: string;
  label: string;
  width: number;
  align?: 'start' | 'center';
}

export interface KeyboardKeyTarget {
  display: string;
  description: string;
  rule: MappingRule;
}

export interface KeyboardCombo {
  id: string;
  trigger: string;
  output: string;
  description: string;
}

export interface KeyboardLayerView {
  id: KeyboardLayerId;
  label: string;
  modifierKey: string;
  title: string;
  description: string;
  tips: string[];
  keyTargets: Record<string, KeyboardKeyTarget>;
  combos: KeyboardCombo[];
}

interface LayerConfig {
  id: KeyboardLayerId;
  label: string;
  modifier: string;
  modifierKey: string;
  title: string;
  description: string;
  tips: string[];
}

function getLayerConfigs(language: AppLanguageId): LayerConfig[] {
  if (language === 'en') {
    return [
      {
        id: 'space',
        label: 'Space Mode',
        modifier: 'Space',
        modifierKey: 'Space',
        title: 'Space Layer',
        description: 'Turns the letter block into navigation, deletion, return, and tab switching.',
        tips: ['J K I L handle arrows', 'H/U/O/N jump by line and word', 'Q/W/E/R/A/S cover edit actions'],
      },
      {
        id: 'number',
        label: '3 Mode',
        modifier: '3',
        modifierKey: '3',
        title: 'Number Layer',
        description: 'Hold 3 to turn the right-hand letter block into a numpad.',
        tips: ['N M , become 1 2 3', 'J K L and U I O cover 4 through 9', 'B and H provide 0'],
      },
      {
        id: 'symbol',
        label: '; Mode',
        modifier: ';',
        modifierKey: ';',
        title: 'Symbol Layer',
        description: 'Hold ; for high-frequency symbols, or tap ; to enter bracket combos.',
        tips: ['Hold ; + letter for symbols', 'Tap ; then XK / ZK / DK for bracket pairs'],
      },
      {
        id: 'mouse',
        label: 'Tab Mode',
        modifier: 'Tab',
        modifierKey: 'Tab',
        title: 'Mouse Layer',
        description: 'Hold Tab for mouse control: JKLI move, U/O scroll, R back, C toggles sidebar, N/M click.',
        tips: ['Movement is faster while Tab is held', 'Movement slows after release', 'R sends the mouse back button', 'C sends Cmd+[ to toggle sidebars', 'N exits after left click, M keeps the mode after right click'],
      },
    ];
  }

  return [
    {
      id: 'space',
      label: 'Space + 模式',
      modifier: 'Space',
      modifierKey: 'Space',
      title: '空格层',
      description: '把主字母区变成方向、删除、换行和标签切换。',
      tips: ['J K I L 负责方向', 'H/U/O/N 负责整词与行首行尾跳转', 'Q/W/E/R/A/S 是编辑动作'],
    },
    {
      id: 'number',
      label: '3 + 模式',
      modifier: '3',
      modifierKey: '3',
      title: '数字层',
      description: '按住 3，把右手字母区直接变成数字小键盘。',
      tips: ['N M , 是 1 2 3', 'J K L / U I O 承担 4 到 9', 'B / H 双入口补 0'],
    },
    {
      id: 'symbol',
      label: '; + 模式',
      modifier: ';',
      modifierKey: ';',
      title: '符号层',
      description: '长按 ; 输入高频符号，轻点 ; 进入括号组合模式。',
      tips: ['长按 ; + 字母 = 高频符号', '轻点 ; 后用 XK / ZK / DK 输出括号对'],
    },
    {
      id: 'mouse',
      label: 'Tab + 模式',
      modifier: 'Tab',
      modifierKey: 'Tab',
      title: '鼠标层',
      description: '按住 Tab 进入鼠标控制：JKLI 移动、U/O 滚动、R 后退、C 开关侧边栏、N/M 点击。',
      tips: ['按住 Tab 时移动更快', '松开 Tab 仍保留方向，但速度会变慢', 'R 发送鼠标后退键', 'C 发送 Cmd+[ 开关侧边栏', 'N 左键后退出，M 右键后保留模式'],
    },
  ];
}

export const KEYBOARD_LAYOUT_ROWS: KeyboardLayoutKey[][] = [
  [
    { id: '`', label: '`', width: 1.1 },
    { id: '1', label: '1', width: 1 },
    { id: '2', label: '2', width: 1 },
    { id: '3', label: '3', width: 1 },
    { id: '4', label: '4', width: 1 },
    { id: '5', label: '5', width: 1 },
    { id: '6', label: '6', width: 1 },
    { id: '7', label: '7', width: 1 },
    { id: '8', label: '8', width: 1 },
    { id: '9', label: '9', width: 1 },
    { id: '0', label: '0', width: 1 },
    { id: '-', label: '-', width: 1 },
    { id: '=', label: '=', width: 1 },
    { id: 'Backspace', label: 'Delete', width: 2.3 },
  ],
  [
    { id: 'Tab', label: 'Tab', width: 1.7 },
    { id: 'Q', label: 'Q', width: 1 },
    { id: 'W', label: 'W', width: 1 },
    { id: 'E', label: 'E', width: 1 },
    { id: 'R', label: 'R', width: 1 },
    { id: 'T', label: 'T', width: 1 },
    { id: 'Y', label: 'Y', width: 1 },
    { id: 'U', label: 'U', width: 1 },
    { id: 'I', label: 'I', width: 1 },
    { id: 'O', label: 'O', width: 1 },
    { id: 'P', label: 'P', width: 1 },
    { id: '[', label: '[', width: 1 },
    { id: ']', label: ']', width: 1 },
    { id: '\\', label: '\\', width: 1.6 },
  ],
  [
    { id: 'Caps', label: 'Caps', width: 2 },
    { id: 'A', label: 'A', width: 1 },
    { id: 'S', label: 'S', width: 1 },
    { id: 'D', label: 'D', width: 1 },
    { id: 'F', label: 'F', width: 1 },
    { id: 'G', label: 'G', width: 1 },
    { id: 'H', label: 'H', width: 1 },
    { id: 'J', label: 'J', width: 1 },
    { id: 'K', label: 'K', width: 1 },
    { id: 'L', label: 'L', width: 1 },
    { id: ';', label: ';', width: 1 },
    { id: "'", label: "'", width: 1 },
    { id: 'Enter', label: 'Enter', width: 2.2 },
  ],
  [
    { id: 'Shift', label: 'Shift', width: 2.5 },
    { id: 'Z', label: 'Z', width: 1 },
    { id: 'X', label: 'X', width: 1 },
    { id: 'C', label: 'C', width: 1 },
    { id: 'V', label: 'V', width: 1 },
    { id: 'B', label: 'B', width: 1 },
    { id: 'N', label: 'N', width: 1 },
    { id: 'M', label: 'M', width: 1 },
    { id: ',', label: ',', width: 1 },
    { id: '.', label: '.', width: 1 },
    { id: '/', label: '/', width: 1 },
    { id: 'ShiftRight', label: 'Shift', width: 2.7 },
  ],
  [
    { id: 'Ctrl', label: 'Ctrl', width: 1.5 },
    { id: 'Opt', label: 'Opt', width: 1.5 },
    { id: 'Cmd', label: 'Cmd', width: 1.6 },
    { id: 'Space', label: 'Space', width: 7.3 },
    { id: 'CmdRight', label: 'Cmd', width: 1.6 },
    { id: 'OptRight', label: 'Opt', width: 1.5 },
    { id: 'Fn', label: 'Fn', width: 1.5 },
  ],
];

function isTapComboRule(rule: MappingRule): boolean {
  return rule.modifier === '; (tap)';
}

function getKeyboardCaption(rule: MappingRule, language: AppLanguageId): string {
  if (language === 'en') {
    switch (rule.id) {
      case 'nav-j':
        return 'Left';
      case 'nav-k':
        return 'Down';
      case 'nav-i':
        return 'Up';
      case 'nav-l':
        return 'Right';
      case 'nav-h':
        return 'Line Start';
      case 'nav-n':
        return 'Line End';
      case 'nav-u':
        return 'Word Left';
      case 'nav-o':
        return 'Word Right';
      case 'edit-q':
        return '4 Spaces';
      case 'edit-w':
        return 'Delete Line';
      case 'edit-e':
        return 'Backspace';
      case 'edit-r':
        return 'Delete Word';
      case 'edit-a':
        return 'Return';
      case 'edit-s':
        return 'Escape';
      case 'sys-z':
        return 'Prev Tab';
      case 'sys-v':
        return 'Next Tab';
      case 'mouse-tab-j':
        return 'Mouse Left';
      case 'mouse-tab-k':
        return 'Mouse Down';
      case 'mouse-tab-i':
        return 'Mouse Up';
      case 'mouse-tab-l':
        return 'Mouse Right';
      case 'mouse-tab-u':
        return 'Scroll Up';
      case 'mouse-tab-o':
        return 'Scroll Down';
      case 'mouse-tab-r':
        return 'Mouse Back';
      case 'mouse-tab-c':
        return 'Toggle Sidebar';
      case 'mouse-tab-n':
        return 'Left Click';
      case 'mouse-tab-m':
        return 'Right Click';
      default:
        return rule.to;
    }
  }

  switch (rule.id) {
    case 'nav-j':
      return '左移';
    case 'nav-k':
      return '下移';
    case 'nav-i':
      return '上移';
    case 'nav-l':
      return '右移';
    case 'nav-h':
      return '行首';
    case 'nav-n':
      return '行尾';
    case 'nav-u':
      return '前一词';
    case 'nav-o':
      return '后一词';
    case 'edit-q':
      return '4 空格';
    case 'edit-w':
      return '删整行';
    case 'edit-e':
      return '退格';
    case 'edit-r':
      return '删一词';
    case 'edit-a':
      return '回车';
    case 'edit-s':
      return '退出';
    case 'sys-z':
      return '上个标签';
    case 'sys-v':
      return '下个标签';
    case 'mouse-tab-j':
      return '鼠标左移';
    case 'mouse-tab-k':
      return '鼠标下移';
    case 'mouse-tab-i':
      return '鼠标上移';
    case 'mouse-tab-l':
      return '鼠标右移';
    case 'mouse-tab-u':
      return '向上滚动';
    case 'mouse-tab-o':
      return '向下滚动';
    case 'mouse-tab-r':
      return '鼠标后退';
    case 'mouse-tab-c':
      return '开关侧边栏';
    case 'mouse-tab-n':
      return '左键点击';
    case 'mouse-tab-m':
      return '右键点击';
    default:
      return rule.to;
  }
}

function createKeyTargets(
  rules: MappingRule[],
  language: AppLanguageId,
): Record<string, KeyboardKeyTarget> {
  return rules.reduce<Record<string, KeyboardKeyTarget>>((targets, rule) => {
    targets[rule.fromDisplay] = {
      display: rule.toDisplay,
      description: getKeyboardCaption(rule, language),
      rule,
    };
    return targets;
  }, {});
}

export function getLocalizedRuleTexts(
  rule: MappingRule,
  language: AppLanguageId,
): {
  toDisplay: string;
  to: string;
} {
  if (rule.id === 'win-alt-tab') {
    return language === 'en'
      ? { toDisplay: 'App Switcher', to: 'Window Switcher' }
      : { toDisplay: '窗口切换', to: '窗口切换器' };
  }

  return {
    toDisplay: rule.toDisplay,
    to: rule.to,
  };
}

export function buildKeyboardLayerViews(
  rules: MappingRule[],
  language: AppLanguageId,
): {
  layers: KeyboardLayerView[];
  otherRules: MappingRule[];
} {
  const activeRules = rules.filter((rule) => rule.active);
  const layerRuleIds = new Set<string>();
  const layerConfigs = getLayerConfigs(language);

  const layers = layerConfigs.map((config) => {
    const scopedRules = activeRules.filter((rule) => rule.modifier === config.modifier);
    const combos = config.id === 'symbol'
      ? activeRules
          .filter(isTapComboRule)
          .map<KeyboardCombo>((rule) => ({
            id: rule.id,
            trigger: rule.fromDisplay,
            output: rule.toDisplay,
            description: rule.to,
          }))
      : [];

    const keyRules = scopedRules.filter((rule) => !isTapComboRule(rule));
    keyRules.forEach((rule) => layerRuleIds.add(rule.id));
    combos.forEach((combo) => layerRuleIds.add(combo.id));

    return {
      id: config.id,
      label: config.label,
      modifierKey: config.modifierKey,
      title: config.title,
      description: config.description,
      tips: config.tips,
      keyTargets: createKeyTargets(keyRules, language),
      combos,
    };
  });

  const otherRules = activeRules.filter((rule) => !layerRuleIds.has(rule.id));

  return { layers, otherRules };
}
