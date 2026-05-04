import type { AppLanguageId, AppThemeId, ModuleName } from '../types';

export interface LanguageOption {
  id: AppLanguageId;
  label: string;
  shortLabel: string;
}

export const DEFAULT_LANGUAGE: AppLanguageId = 'en';
export const LANGUAGE_STORAGE_KEY = 'yorling.language';

export const LANGUAGE_OPTIONS: LanguageOption[] = [
  { id: 'zh', label: '中文', shortLabel: '中' },
  { id: 'en', label: 'English', shortLabel: 'EN' },
];

const THEME_DESCRIPTIONS: Record<AppLanguageId, Record<AppThemeId, string>> = {
  zh: {
    light: '温暖明亮的浅色主题。',
    dark: '沉稳温暖的深色主题。',
  },
  en: {
    light: 'Warm, clean light canvas.',
    dark: 'Warm, refined dark surface.',
  },
};

const MODULE_LABELS: Record<AppLanguageId, Record<ModuleName, string>> = {
  zh: {
    keyboard: '映射',
    music: '音乐',
    superRightClick: '超级右键',
    clipboard: '剪贴板',
    island: '灵动岛',
    terminals: '多终端',
  },
  en: {
    keyboard: 'Mapping',
    music: 'Music',
    superRightClick: 'Super Right Click',
    clipboard: 'Clipboard',
    island: 'Island',
    terminals: 'Terminals',
  },
};

type UiCopy = {
  titlebar: {
    windowTitle: string;
    themeSwitcherLabel: string;
    languageSwitcherLabel: string;
    closeWindow: string;
    minimizeWindow: string;
    zoomWindow: string;
    autoLaunchLabel: string;
    autoLaunchOn: string;
    autoLaunchOff: string;
    autoLaunchUnavailable: string;
    enableAutoLaunch: string;
    disableAutoLaunch: string;
    updateLabel: string;
    checkForUpdates: string;
    checkingForUpdates: string;
    installUpdate: string;
    installingUpdate: string;
    upToDate: string;
    updateNotConfigured: string;
    updateRetry: string;
    updateAvailableTitle: (version: string) => string;
  };
  sidebar: {
    modules: Record<ModuleName, string>;
    soonBadge: string;
    activeStatus: string;
    inactiveStatus: string;
  };
  statusbar: {
    accessibility: string;
    screenRecording: string;
    granted: string;
    missing: string;
    openAccessibilitySettings: string;
    openScreenRecordingSettings: string;
  };
  keyboard: {
    enabled: string;
    paused: string;
    start: string;
    stop: string;
    permissionTitle: string;
    permissionDescription: string;
    permissionAction: string;
    selectedLayer: string;
    liveLayer: string;
    activeLayerHint: string;
    tapCombos: string;
    otherMappings: string;
    otherMappingsEnabled: string;
    otherMappingsPaused: string;
    toggleOtherMappings: string;
    rulesLabel: (count: number) => string;
    hold: string;
    currentLayerBadge: string;
    modeTag: string;
  };
  placeholder: {
    comingSoon: string;
  };
  music: {
    title: string;
    subtitle: string;
    description: string;
    layoutLabel: string;
    classicLayout: string;
    pianoLayout: string;
    octaveLabel: string;
    tutorialLabel: string;
    tutorialOn: string;
    tutorialOff: string;
    scorePerfect: string;
    scoreGood: string;
    scoreMiss: string;
    scoreCombo: string;
    scoreAccuracy: string;
    scoreResults: string;
    scoreMaxCombo: string;
    scorePlayAgain: string;
    scoreChangeSong: string;
    scoreTotalNotes: string;
    scoreRank: string;
    browseSongs: string;
    closeSongBrowser: string;
    recentSongsTitle: string;
    libraryTitle: string;
    recentSongBadge: string;
    searchSongs: string;
    searchSongsPlaceholder: string;
    searchNoResults: string;
    searchTryAnother: string;
    randomSong: string;
    randomChain: string;
    demoPlay: string;
    demoPlaying: string;
    demoStop: string;
  };
  island: {
    title: string;
    subtitle: string;
    description: string;
    settingsSubtitle: string;
    configPathLabel: string;
    providersTitle: string;
    loadingProviders: string;
    installHook: string;
    reinstallHook: string;
    uninstallHook: string;
    hookInstalled: string;
    hookNotInstalled: string;
    hookOutdated: string;
    hookBroken: string;
    howItWorksTitle: string;
    howItWorksBody: string;
    enableToggle: string;
    enabled: string;
    disabled: string;
  };
  superRightClick: {
    title: string;
    subtitle: string;
    start: string;
    stop: string;
    enabled: string;
    disabled: string;
    permissionTitle: string;
    permissionDescription: string;
    permissionAction: string;
    nativeEnabled: string;
    nativeDisabled: string;
    nativeMissing: string;
    nativeSettingsAction: string;
    permissionRecoveryTitle: string;
    permissionRecoveryDescription: string;
    coverageTitle: string;
    coverageDescription: string;
    systemEventsPermissionAction: string;
    accessibilitySettingsAction: string;
    terminalTitle: string;
    terminalLabel: string;
    noTerminal: string;
    refresh: string;
    actionsTitle: string;
    actionNewTxt: string;
    actionNewMd: string;
    actionOpenTerminal: string;
    actionCopyPath: string;
    actionMoveTo: string;
    actionToggleHidden: string;
    actionSnapToGrid: string;
    running: string;
    idle: string;
    eventCount: (count: number) => string;
    launchErrorPrefix: string;
    menuSurfaceDesktop: string;
    menuSurfaceFolder: string;
  };
  clipboard: {
    title: string;
    subtitle: string;
    enabled: string;
    disabled: string;
    refresh: string;
    clear: string;
    copied: string;
    copyItem: string;
    copyErrorPrefix: string;
    searchPlaceholder: string;
    searchNoResults: string;
    pinItem: string;
    unpinItem: string;
    pinnedBadge: string;
    deleteItem: string;
    deleteErrorPrefix: string;
    pinErrorPrefix: string;
    openImageLocation: string;
    imageLocationUnavailable: string;
    openFileLocation: string;
    fileLocationUnavailable: string;
    openLocationErrorPrefix: string;
    emptyTitle: string;
    emptyDescription: string;
    textBadge: string;
    imageBadge: string;
    fileBadge: string;
    folderBadge: string;
    fileGroupBadge: string;
    imageGroupBadge: string;
    unsupportedBadge: string;
    itemsCount: (count: number) => string;
    chars: (count: number) => string;
    lines: (count: number) => string;
    imageSize: (width: number, height: number) => string;
  };
  terminals: {
    title: string;
    subtitle: string;
    addProject: string;
    addAgent: string;
    directoryPickerTitle: string;
    pathInputLabel: string;
    pathPlaceholder: string;
    projectsTitle: string;
    openAgentsTitle: string;
    allProjects: string;
    emptyTitle: string;
    emptyDescription: string;
    selectedPath: string;
    agentLabel: string;
    terminalLabel: string;
    embeddedTerminal: string;
    agentRemembered: string;
    launch: string;
    launching: string;
    launchSuccess: string;
    launchErrorPrefix: string;
    actionErrorPrefix: string;
    removeProject: string;
    removeProjectSuccess: string;
    removeProjectConfirmTitle: string;
    removeProjectConfirmMessage: (name: string) => string;
    removeProjectConfirmConfirm: string;
    removeProjectConfirmCancel: string;
    pathPrompt: string;
    expand: string;
    collapse: string;
    noProject: string;
    noProjectSelectedDescription: string;
    noOpenAgents: string;
    noOpenAgentsInProject: string;
    promptHistoryToggle: string;
    promptHistoryOpen: string;
    promptHistoryClose: string;
    promptHistoryTitle: string;
    promptHistoryRecent: (limit: number) => string;
    promptHistoryEmptyTitle: string;
    promptHistoryEmptyDescription: string;
    promptHistoryCopy: string;
    promptHistoryCopied: string;
    terminalPanelTitle: string;
    terminalPanelIdle: string;
    terminalPanelIdleDescription: string;
    terminalStop: string;
    runningTerminal: string;
    readyTerminal: string;
    lastLaunched: string;
    neverLaunched: string;
    configuredAgents: (count: number) => string;
  };
};

const UI_COPY: Record<AppLanguageId, UiCopy> = {
  zh: {
    titlebar: {
      windowTitle: 'YORLING',
      themeSwitcherLabel: '主题切换',
      languageSwitcherLabel: '语言切换',
      closeWindow: '关闭窗口',
      minimizeWindow: '最小化窗口',
      zoomWindow: '缩放窗口',
      autoLaunchLabel: '开机自启',
      autoLaunchOn: '已开',
      autoLaunchOff: '已关',
      autoLaunchUnavailable: '不可用',
      enableAutoLaunch: '开启开机自启',
      disableAutoLaunch: '关闭开机自启',
      updateLabel: '更新',
      checkForUpdates: '检查更新',
      checkingForUpdates: '检查中',
      installUpdate: '安装更新',
      installingUpdate: '安装中',
      upToDate: '已最新',
      updateNotConfigured: '未配置',
      updateRetry: '重试更新',
      updateAvailableTitle: (version) => `发现 Yorling ${version}，点击安装`,
    },
    sidebar: {
      modules: MODULE_LABELS.zh,
      soonBadge: '即将推出',
      activeStatus: '运行中',
      inactiveStatus: '未运行',
    },
    statusbar: {
      accessibility: '辅助功能',
      screenRecording: '屏幕录制',
      granted: '已授权',
      missing: '未授权',
      openAccessibilitySettings: '打开辅助功能设置',
      openScreenRecordingSettings: '打开屏幕录制设置',
    },
    keyboard: {
      enabled: '启用',
      paused: '暂停',
      start: '启动',
      stop: '停止',
      permissionTitle: '需要辅助功能权限',
      permissionDescription: 'Yorling 需要系统辅助功能权限才能接管键盘事件。授权后如未生效，请重启应用。',
      permissionAction: '前往授权',
      selectedLayer: '当前层',
      liveLayer: '实时层',
      activeLayerHint: '高亮按键即当前修饰键控制的映射层',
      tapCombos: '轻点组合',
      otherMappings: '其他映射',
      otherMappingsEnabled: '全部开启',
      otherMappingsPaused: '全部暂停',
      toggleOtherMappings: '切换全部其他映射',
      rulesLabel: (count) => `${count} 条规则`,
      hold: '长按',
      currentLayerBadge: '当前层',
      modeTag: '模式',
    },
    placeholder: {
      comingSoon: '即将推出',
    },
    music: {
      title: '键盘音乐',
      subtitle: '用键盘触发旋律与节奏',
      description: '把键盘变成一套可演奏的乐器界面。',
      layoutLabel: '音键布局',
      classicLayout: '原布局',
      pianoLayout: '钢琴布局',
      octaveLabel: '当前八度',
      tutorialLabel: '音游',
      tutorialOn: '开启',
      tutorialOff: '关闭',
      scorePerfect: 'Perfect',
      scoreGood: 'Good',
      scoreMiss: 'Miss',
      scoreCombo: '连击',
      scoreAccuracy: '准确率',
      scoreResults: '演奏结果',
      scoreMaxCombo: '最大连击',
      scorePlayAgain: '再来一次',
      scoreChangeSong: '更换曲目',
      scoreTotalNotes: '总音符',
      scoreRank: '评级',
      browseSongs: '浏览曲库',
       closeSongBrowser: '关闭曲库',
       recentSongsTitle: '最近曲目',
       libraryTitle: '全部曲库',
       recentSongBadge: '最近练过',
       searchSongs: '搜索曲目',
       searchSongsPlaceholder: '搜索曲名、难度或分类',
       searchNoResults: '没有找到匹配的曲目',
       searchTryAnother: '试试更短的关键词，或换成曲名、难度与分类。',
       randomSong: '随机曲目',
       randomChain: '播完继续随机',
       demoPlay: '演示播放',
       demoPlaying: '演示中',
       demoStop: '停止演示',
    },
    island: {
      title: '灵动岛',
      subtitle: '在屏幕顶部追踪 AI Agent 状态',
      description: '在顶部显示 Agent 的实时状态。',
      settingsSubtitle: '管理 Agent 连接和 Hook 安装',
      configPathLabel: '配置位置',
      providersTitle: 'Agent 连接',
      loadingProviders: '正在加载...',
      installHook: '安装 Hook',
      reinstallHook: '重新安装',
      uninstallHook: '卸载 Hook',
      hookInstalled: '已安装',
      hookNotInstalled: '未安装',
      hookOutdated: '需要更新',
      hookBroken: '已损坏',
      howItWorksTitle: '工作原理',
      howItWorksBody: '灵动岛通过在 Agent 的配置文件中安装 Hook 来接收实时事件。安装后，Agent 的每次工具调用、权限请求和状态变化都会显示在屏幕顶部的浮窗中。你可以直接在灵动岛中批准权限请求或回答 Agent 的问题。',
      enableToggle: '启用灵动岛',
      enabled: '已启用',
      disabled: '已关闭',
    },
    superRightClick: {
      title: '超级右键',
      subtitle: 'Finder 与桌面空白处的快速动作菜单',
      start: '启动',
      stop: '停止',
      enabled: '已启用',
      disabled: '未启用',
      permissionTitle: '需要辅助功能权限',
      permissionDescription: '超级右键只通过 FinderSync 原生扩展接入 Finder 右键菜单。',
      permissionAction: '前往授权',
      nativeEnabled: '系统已启用',
      nativeDisabled: '已嵌入，待系统启用',
      nativeMissing: '未在当前 App 中嵌入',
      nativeSettingsAction: '打开系统扩展设置',
      permissionRecoveryTitle: '权限恢复入口',
      permissionRecoveryDescription: '如果刚才误点了不允许，可以从这里重新打开 macOS 授权页。',
      coverageTitle: 'Finder 覆盖范围',
      coverageDescription: '菜单会覆盖真实文件系统位置，包括个人目录、应用程序、系统应用、外接磁盘与根目录。最近使用、搜索结果、隔空投送等 Finder 智能位置没有稳定的目标文件夹，空白处右键可能只显示系统菜单；选中真实文件后仍会尽量显示可用动作。',
      systemEventsPermissionAction: '打开 System Events 自动化权限',
      accessibilitySettingsAction: '打开辅助功能权限',
      terminalTitle: '终端选择',
      terminalLabel: '右键菜单默认终端',
      noTerminal: '未检测到可用终端',
      refresh: '刷新',
      actionsTitle: '菜单动作',
      actionNewTxt: '新建 TXT 文件',
      actionNewMd: '新建 Markdown 文件',
      actionOpenTerminal: '打开终端',
      actionCopyPath: '复制当前文件夹路径',
      actionMoveTo: '移动到其他文件夹',
      actionToggleHidden: '显示/隐藏隐藏文件',
      actionSnapToGrid: '吸附到网格',
      running: '监听中',
      idle: '已停止',
      eventCount: (count) => `${count} 次右键`,
      launchErrorPrefix: '启动失败',
      menuSurfaceDesktop: '桌面',
      menuSurfaceFolder: '文件夹',
    },
    clipboard: {
      title: '剪贴板',
      subtitle: '最近复制的文字、图片和文件，会像便签一样自然散落在这里。',
      enabled: '监听中',
      disabled: '已暂停',
      refresh: '刷新',
      clear: '清空',
      copied: '已放回剪贴板',
      copyItem: '复制这一条',
      copyErrorPrefix: '复制失败',
      searchPlaceholder: '搜索文字、文件名或路径',
      searchNoResults: '没有匹配的剪贴记录',
      pinItem: '置顶',
      unpinItem: '取消置顶',
      pinnedBadge: '置顶',
      deleteItem: '删除',
      deleteErrorPrefix: '删除失败',
      pinErrorPrefix: '置顶失败',
      openImageLocation: '打开图片所在位置',
      imageLocationUnavailable: '这张图片没有本地位置',
      openFileLocation: '在 Finder 中显示',
      fileLocationUnavailable: '这条记录没有本地位置',
      openLocationErrorPrefix: '打开位置失败',
      emptyTitle: '还没有剪贴痕迹',
      emptyDescription: '复制文字、图片、文件或文件夹后，它会自动出现在这里。',
      textBadge: '文字',
      imageBadge: '图片',
      fileBadge: '文件',
      folderBadge: '文件夹',
      fileGroupBadge: '文件组',
      imageGroupBadge: '图片组',
      unsupportedBadge: '内容',
      itemsCount: (count) => `${count} 条`,
      chars: (count) => `${count} 字符`,
      lines: (count) => `${count} 行`,
      imageSize: (width, height) => `${Math.round(width)} × ${Math.round(height)}`,
    },
    terminals: {
      title: '多终端',
      subtitle: '按项目路径启动对应的 Agent 终端',
      addProject: '添加项目',
      addAgent: '添加 Agent',
      directoryPickerTitle: '选择项目文件夹',
      pathInputLabel: '项目路径',
      pathPlaceholder: '/Users/example/project',
      projectsTitle: '项目',
      openAgentsTitle: '已打开 Agent',
      allProjects: '全部项目',
      emptyTitle: '还没有项目',
      emptyDescription: '添加一个文件夹路径后，就能为它选择默认 Agent。',
      selectedPath: '当前路径',
      agentLabel: '启动 Agent',
      terminalLabel: '启动终端',
      embeddedTerminal: '默认（内置）',
      agentRemembered: '此路径会记住当前 Agent',
      launch: '启动终端',
      launching: '启动中...',
      launchSuccess: '已打开终端',
      launchErrorPrefix: '启动失败',
      actionErrorPrefix: '操作失败',
      removeProject: '移除项目',
      removeProjectSuccess: '已移除项目',
      removeProjectConfirmTitle: '删除这个项目？',
      removeProjectConfirmMessage: (name) => `移除后将不再在多终端里显示“${name}”及其提示词历史。`,
      removeProjectConfirmConfirm: '删除',
      removeProjectConfirmCancel: '取消',
      pathPrompt: '输入项目文件夹路径',
      expand: '展开多终端项目',
      collapse: '收起多终端项目',
      noProject: '未选择项目',
      noProjectSelectedDescription: '点击左侧项目可以筛选它对应的已打开 Agent，也可以直接停留在“多终端”查看全部已打开 Agent。',
      noOpenAgents: '当前还没有已打开的 Agent。',
      noOpenAgentsInProject: '这个项目下还没有已打开的 Agent。',
      promptHistoryToggle: '提示词',
      promptHistoryOpen: '打开提示词历史',
      promptHistoryClose: '关闭提示词历史',
      promptHistoryTitle: '项目提示词',
      promptHistoryRecent: (limit) => `最近 ${limit} 条提示词`,
      promptHistoryEmptyTitle: '还没有提示词',
      promptHistoryEmptyDescription: '这个项目一旦在 Yorling、终端或外部 Agent 应用里产生用户提示词，就会自动汇总到这里。',
      promptHistoryCopy: '复制',
      promptHistoryCopied: '已复制',
      terminalPanelTitle: '终端',
      terminalPanelIdle: '内置终端待启动',
      terminalPanelIdleDescription: '选择默认（内置）并点击 + 后，会在这里显示这个项目的 Agent 终端。',
      terminalStop: '停止',
      runningTerminal: '运行中',
      readyTerminal: '就绪',
      lastLaunched: '上次启动',
      neverLaunched: '尚未启动',
      configuredAgents: (count) => `${count} 个 Agent`,
    },
  },
  en: {
    titlebar: {
      windowTitle: 'YORLING',
      themeSwitcherLabel: 'Theme switcher',
      languageSwitcherLabel: 'Language switcher',
      closeWindow: 'Close window',
      minimizeWindow: 'Minimize window',
      zoomWindow: 'Zoom window',
      autoLaunchLabel: 'Launch at login',
      autoLaunchOn: 'On',
      autoLaunchOff: 'Off',
      autoLaunchUnavailable: 'Unavailable',
      enableAutoLaunch: 'Enable launch at login',
      disableAutoLaunch: 'Disable launch at login',
      updateLabel: 'Update',
      checkForUpdates: 'Check for updates',
      checkingForUpdates: 'Checking',
      installUpdate: 'Install update',
      installingUpdate: 'Installing',
      upToDate: 'Current',
      updateNotConfigured: 'Setup needed',
      updateRetry: 'Retry update',
      updateAvailableTitle: (version) => `Yorling ${version} is available. Click to install.`,
    },
    sidebar: {
      modules: MODULE_LABELS.en,
      soonBadge: 'Soon',
      activeStatus: 'Active',
      inactiveStatus: 'Inactive',
    },
    statusbar: {
      accessibility: 'Accessibility',
      screenRecording: 'Screen Recording',
      granted: 'Granted',
      missing: 'Missing',
      openAccessibilitySettings: 'Open Accessibility settings',
      openScreenRecordingSettings: 'Open Screen Recording settings',
    },
    keyboard: {
      enabled: 'Enabled',
      paused: 'Paused',
      start: 'Start',
      stop: 'Stop',
      permissionTitle: 'Accessibility permission required',
      permissionDescription: 'Yorling needs macOS Accessibility permission to intercept keyboard events. Restart the app if the permission does not take effect right away.',
      permissionAction: 'Grant access',
      selectedLayer: 'Selected layer',
      liveLayer: 'Live layer',
      activeLayerHint: 'Highlighted keys belong to the mapping layer controlled by this modifier.',
      tapCombos: 'Tap combos',
      otherMappings: 'Other mappings',
      otherMappingsEnabled: 'All on',
      otherMappingsPaused: 'All off',
      toggleOtherMappings: 'Toggle all other mappings',
      rulesLabel: (count) => `${count} rules`,
      hold: 'Hold',
      currentLayerBadge: 'Current',
      modeTag: 'Mode',
    },
    placeholder: {
      comingSoon: 'Coming soon',
    },
    music: {
      title: 'Keyboard Music',
      subtitle: 'Turn your keyboard into a playable instrument',
      description: 'A music module designed for rhythm, notes, and performance shortcuts.',
      layoutLabel: 'Key layout',
      classicLayout: 'Classic',
      pianoLayout: 'Piano',
      octaveLabel: 'Octave',
      tutorialLabel: 'Rhythm Game',
      tutorialOn: 'On',
      tutorialOff: 'Off',
      scorePerfect: 'Perfect',
      scoreGood: 'Good',
      scoreMiss: 'Miss',
      scoreCombo: 'Combo',
      scoreAccuracy: 'Accuracy',
      scoreResults: 'Results',
      scoreMaxCombo: 'Max Combo',
      scorePlayAgain: 'Play Again',
      scoreChangeSong: 'Change Song',
      scoreTotalNotes: 'Total Notes',
      scoreRank: 'Rank',
      browseSongs: 'Browse songs',
       closeSongBrowser: 'Close song browser',
       recentSongsTitle: 'Recent songs',
       libraryTitle: 'Full library',
       recentSongBadge: 'Recent',
       searchSongs: 'Search songs',
       searchSongsPlaceholder: 'Search titles, difficulty, or categories',
       searchNoResults: 'No matching songs found',
       searchTryAnother: 'Try a shorter term, or search by title, difficulty, or category.',
       randomSong: 'Random',
       randomChain: 'Keep random going',
       demoPlay: 'Demo',
       demoPlaying: 'Demo playing',
       demoStop: 'Stop demo',
    },
    island: {
      title: 'Dynamic Island',
      subtitle: 'Track AI agent activity from the top of your screen',
      description: 'A compact island for live agent status.',
      settingsSubtitle: 'Manage agent connections and hook installation',
      configPathLabel: 'Config path',
      providersTitle: 'Agent Connections',
      loadingProviders: 'Loading...',
      installHook: 'Install Hook',
      reinstallHook: 'Reinstall',
      uninstallHook: 'Uninstall Hook',
      hookInstalled: 'Installed',
      hookNotInstalled: 'Not installed',
      hookOutdated: 'Needs update',
      hookBroken: 'Broken',
      howItWorksTitle: 'How It Works',
      howItWorksBody: 'Dynamic Island receives real-time events by installing hooks into your AI agent\'s configuration. Once installed, every tool call, permission request, and status change is shown in the floating overlay at the top of your screen. You can approve permissions or answer agent questions directly from the island.',
      enableToggle: 'Enable Dynamic Island',
      enabled: 'Enabled',
      disabled: 'Disabled',
    },
    superRightClick: {
      title: 'Super Right Click',
      subtitle: 'Quick actions for Finder and Desktop background menus',
      start: 'Start',
      stop: 'Stop',
      enabled: 'Enabled',
      disabled: 'Disabled',
      permissionTitle: 'Accessibility permission required',
      permissionDescription: 'Super Right Click only integrates through the native FinderSync extension.',
      permissionAction: 'Grant access',
      nativeEnabled: 'Enabled by macOS',
      nativeDisabled: 'Bundled, waiting for enablement',
      nativeMissing: 'Not bundled in this app',
      nativeSettingsAction: 'Open Extensions settings',
      permissionRecoveryTitle: 'Permission Recovery',
      permissionRecoveryDescription: 'If a macOS prompt was denied by mistake, reopen the related permission page here.',
      coverageTitle: 'Finder Coverage',
      coverageDescription: 'The menu covers real file-system locations, including Home, Applications, system apps, external disks, and the root volume. Finder smart locations such as Recents, search results, and AirDrop do not expose a stable target folder, so background right-clicks there may only show the system menu; selected real files still get the available actions where Finder allows it.',
      systemEventsPermissionAction: 'Open System Events automation',
      accessibilitySettingsAction: 'Open Accessibility settings',
      terminalTitle: 'Terminal',
      terminalLabel: 'Default terminal for the menu',
      noTerminal: 'No terminal detected',
      refresh: 'Refresh',
      actionsTitle: 'Menu actions',
      actionNewTxt: 'New TXT file',
      actionNewMd: 'New Markdown file',
      actionOpenTerminal: 'Open terminal',
      actionCopyPath: 'Copy current folder path',
      actionMoveTo: 'Move to another folder',
      actionToggleHidden: 'Show/hide hidden files',
      actionSnapToGrid: 'Snap to grid',
      running: 'Listening',
      idle: 'Stopped',
      eventCount: (count) => `${count} right-click${count === 1 ? '' : 's'}`,
      launchErrorPrefix: 'Start failed',
      menuSurfaceDesktop: 'Desktop',
      menuSurfaceFolder: 'Folder',
    },
    clipboard: {
      title: 'Clipboard',
      subtitle: 'Recent text, images, and files arranged as an ordered wall of notes.',
      enabled: 'Watching',
      disabled: 'Paused',
      refresh: 'Refresh',
      clear: 'Clear',
      copied: 'Copied back to clipboard',
      copyItem: 'Copy this item',
      copyErrorPrefix: 'Copy failed',
      searchPlaceholder: 'Search text, file names, or paths',
      searchNoResults: 'No matching clipboard items',
      pinItem: 'Pin',
      unpinItem: 'Unpin',
      pinnedBadge: 'Pinned',
      deleteItem: 'Delete',
      deleteErrorPrefix: 'Delete failed',
      pinErrorPrefix: 'Pin failed',
      openImageLocation: 'Show image in Finder',
      imageLocationUnavailable: 'No local image location',
      openFileLocation: 'Show in Finder',
      fileLocationUnavailable: 'No local file location',
      openLocationErrorPrefix: 'Open location failed',
      emptyTitle: 'No clippings yet',
      emptyDescription: 'Copy text, images, files, or folders, and they will appear here automatically.',
      textBadge: 'Text',
      imageBadge: 'Image',
      fileBadge: 'File',
      folderBadge: 'Folder',
      fileGroupBadge: 'Files',
      imageGroupBadge: 'Images',
      unsupportedBadge: 'Item',
      itemsCount: (count) => `${count} item${count === 1 ? '' : 's'}`,
      chars: (count) => `${count} chars`,
      lines: (count) => `${count} lines`,
      imageSize: (width, height) => `${Math.round(width)} × ${Math.round(height)}`,
    },
    terminals: {
      title: 'Terminals',
      subtitle: 'Launch an agent terminal from each project path',
      addProject: 'Add project',
      addAgent: 'Add agent',
      directoryPickerTitle: 'Choose a project folder',
      pathInputLabel: 'Project path',
      pathPlaceholder: '/Users/example/project',
      projectsTitle: 'Projects',
      openAgentsTitle: 'Open agents',
      allProjects: 'All projects',
      emptyTitle: 'No projects yet',
      emptyDescription: 'Add a folder path, then choose the default agent for it.',
      selectedPath: 'Current path',
      agentLabel: 'Launch agent',
      terminalLabel: 'Launch terminal',
      embeddedTerminal: 'Default (embedded)',
      agentRemembered: 'This path remembers the current agent',
      launch: 'Launch terminal',
      launching: 'Launching...',
      launchSuccess: 'Terminal opened',
      launchErrorPrefix: 'Launch failed',
      actionErrorPrefix: 'Action failed',
      removeProject: 'Remove project',
      removeProjectSuccess: 'Project removed',
      removeProjectConfirmTitle: 'Delete this project?',
      removeProjectConfirmMessage: (name) => `Remove “${name}” from multi-terminal and clear its saved prompt history.`,
      removeProjectConfirmConfirm: 'Delete',
      removeProjectConfirmCancel: 'Cancel',
      pathPrompt: 'Enter a project folder path',
      expand: 'Expand terminal projects',
      collapse: 'Collapse terminal projects',
      noProject: 'No project selected',
      noProjectSelectedDescription: 'Pick a project on the left to filter its open agents, or stay on Terminals to see every open agent.',
      noOpenAgents: 'There are no open agents yet.',
      noOpenAgentsInProject: 'This project does not have any open agents yet.',
      promptHistoryToggle: 'Prompts',
      promptHistoryOpen: 'Open prompt history',
      promptHistoryClose: 'Close prompt history',
      promptHistoryTitle: 'Project prompts',
      promptHistoryRecent: (limit) => `Latest ${limit} prompts`,
      promptHistoryEmptyTitle: 'No prompts yet',
      promptHistoryEmptyDescription: 'User prompts from Yorling, terminals, and supported external agent apps will gather here for this project.',
      promptHistoryCopy: 'Copy',
      promptHistoryCopied: 'Copied',
      terminalPanelTitle: 'Terminal',
      terminalPanelIdle: 'Embedded terminal is ready',
      terminalPanelIdleDescription: 'Choose Default (embedded), then press + to show this project agent terminal here.',
      terminalStop: 'Stop',
      runningTerminal: 'Running',
      readyTerminal: 'Ready',
      lastLaunched: 'Last launched',
      neverLaunched: 'Never launched',
      configuredAgents: (count) => `${count} agent${count === 1 ? '' : 's'}`,
    },
  },
};

export function isLanguageId(value: string | null | undefined): value is AppLanguageId {
  return value === 'zh' || value === 'en';
}

export function getStoredLanguage(storage?: Pick<Storage, 'getItem'> | null): AppLanguageId {
  if (!storage) {
    return DEFAULT_LANGUAGE;
  }

  const storedLanguage = storage.getItem(LANGUAGE_STORAGE_KEY);
  return isLanguageId(storedLanguage) ? storedLanguage : DEFAULT_LANGUAGE;
}

export function persistLanguage(language: AppLanguageId, storage?: Pick<Storage, 'setItem'> | null): void {
  storage?.setItem(LANGUAGE_STORAGE_KEY, language);
}

export function getUiCopy(language: AppLanguageId): UiCopy {
  return UI_COPY[language];
}

export function getThemeDescription(theme: AppThemeId, language: AppLanguageId): string {
  return THEME_DESCRIPTIONS[language][theme];
}
