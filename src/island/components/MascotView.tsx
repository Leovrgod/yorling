import type { CSSProperties } from 'react';
import type { IslandSessionPhase } from '../../types';
import { phaseToMascotState } from './mascots/types';

import claudeCodeIcon from '../../assets/agent-icons/claude-code.svg';
import codebuddyIcon from '../../assets/agent-icons/codebuddy.svg';
import codexIcon from '../../assets/agent-icons/codex.svg';
import copilotCliIcon from '../../assets/agent-icons/copilot-cli.svg';
import cursorIcon from '../../assets/agent-icons/cursor.svg';
import geminiCliIcon from '../../assets/agent-icons/gemini-cli.svg';
import hermesIcon from '../../assets/agent-icons/hermes.svg';
import openclawIcon from '../../assets/agent-icons/openclaw.svg';
import opencodeIcon from '../../assets/agent-icons/opencode.svg';
import qoderIcon from '../../assets/agent-icons/qoder.svg';
import qoderworkIcon from '../../assets/agent-icons/qoderwork.svg';
import qwenCodeIcon from '../../assets/agent-icons/qwen-code.svg';
import workbuddyIcon from '../../assets/agent-icons/workbuddy.svg';

interface MascotViewProps {
  providerId: string;
  phase: IslandSessionPhase;
  size?: number;
  motion?: boolean;
}

type MascotStyle = CSSProperties & {
  '--mascot-size': string;
  '--mascot-spark-lg': string;
  '--mascot-spark-sm': string;
  '--mascot-orbit': string;
  '--mascot-alert-orbit-start': string;
  '--mascot-alert-orbit-end': string;
  '--mascot-accent': string;
  '--mascot-accent-soft': string;
  '--mascot-accent-glow': string;
  '--mascot-depth': string;
};

interface MascotTheme {
  accent: string;
  accentSoft: string;
  accentGlow: string;
  depth: string;
}

const PROVIDER_ICON_ASSETS: Record<string, string> = {
  'claude-code': claudeCodeIcon,
  codebuddy: codebuddyIcon,
  codex: codexIcon,
  copilot: copilotCliIcon,
  cursor: cursorIcon,
  gemini: geminiCliIcon,
  hermes: hermesIcon,
  openclaw: openclawIcon,
  opencode: opencodeIcon,
  qoder: qoderIcon,
  qoderwork: qoderworkIcon,
  'qwen-code': qwenCodeIcon,
  workbuddy: workbuddyIcon,
};

const DEFAULT_MASCOT_THEME: MascotTheme = {
  accent: 'rgba(255, 255, 255, 0.86)',
  accentSoft: 'rgba(255, 255, 255, 0.22)',
  accentGlow: 'rgba(255, 255, 255, 0.18)',
  depth: 'rgba(255, 255, 255, 0.14)',
};

const PROVIDER_MASCOT_THEMES: Record<string, MascotTheme> = {
  'claude-code': {
    accent: '#ffb089',
    accentSoft: 'rgba(255, 176, 137, 0.28)',
    accentGlow: 'rgba(255, 130, 84, 0.24)',
    depth: 'rgba(143, 73, 45, 0.5)',
  },
  codebuddy: {
    accent: '#86efac',
    accentSoft: 'rgba(134, 239, 172, 0.28)',
    accentGlow: 'rgba(52, 211, 153, 0.22)',
    depth: 'rgba(21, 128, 61, 0.44)',
  },
  codex: {
    accent: '#9fe6d1',
    accentSoft: 'rgba(159, 230, 209, 0.26)',
    accentGlow: 'rgba(45, 212, 191, 0.2)',
    depth: 'rgba(20, 184, 166, 0.38)',
  },
  copilot: {
    accent: '#ff7eae',
    accentSoft: 'rgba(255, 126, 174, 0.28)',
    accentGlow: 'rgba(244, 63, 94, 0.24)',
    depth: 'rgba(157, 23, 77, 0.42)',
  },
  cursor: {
    accent: '#d9d0b8',
    accentSoft: 'rgba(217, 208, 184, 0.24)',
    accentGlow: 'rgba(250, 250, 245, 0.18)',
    depth: 'rgba(84, 77, 63, 0.52)',
  },
  gemini: {
    accent: '#b7a8ff',
    accentSoft: 'rgba(183, 168, 255, 0.28)',
    accentGlow: 'rgba(96, 165, 250, 0.22)',
    depth: 'rgba(76, 29, 149, 0.44)',
  },
  hermes: {
    accent: '#f9d36a',
    accentSoft: 'rgba(249, 211, 106, 0.28)',
    accentGlow: 'rgba(251, 191, 36, 0.22)',
    depth: 'rgba(146, 64, 14, 0.44)',
  },
  openclaw: {
    accent: '#fb7185',
    accentSoft: 'rgba(251, 113, 133, 0.28)',
    accentGlow: 'rgba(248, 113, 113, 0.22)',
    depth: 'rgba(153, 27, 27, 0.44)',
  },
  opencode: {
    accent: '#93c5fd',
    accentSoft: 'rgba(147, 197, 253, 0.28)',
    accentGlow: 'rgba(59, 130, 246, 0.2)',
    depth: 'rgba(30, 64, 175, 0.4)',
  },
  qoder: {
    accent: '#c4b5fd',
    accentSoft: 'rgba(196, 181, 253, 0.28)',
    accentGlow: 'rgba(139, 92, 246, 0.22)',
    depth: 'rgba(91, 33, 182, 0.42)',
  },
  qoderwork: {
    accent: '#67e8f9',
    accentSoft: 'rgba(103, 232, 249, 0.26)',
    accentGlow: 'rgba(34, 211, 238, 0.2)',
    depth: 'rgba(14, 116, 144, 0.42)',
  },
  'qwen-code': {
    accent: '#a7f3d0',
    accentSoft: 'rgba(167, 243, 208, 0.26)',
    accentGlow: 'rgba(16, 185, 129, 0.2)',
    depth: 'rgba(6, 95, 70, 0.42)',
  },
  workbuddy: {
    accent: '#fdba74',
    accentSoft: 'rgba(253, 186, 116, 0.28)',
    accentGlow: 'rgba(249, 115, 22, 0.2)',
    depth: 'rgba(154, 52, 18, 0.42)',
  },
};

/**
 * Routes a provider ID to a bundled SVG brand mark.
 * Falls back to a letter initial for unknown providers.
 */
export function MascotView({ providerId, phase, size = 28, motion = false }: MascotViewProps) {
  const state = phaseToMascotState(phase);
  const iconSrc = PROVIDER_ICON_ASSETS[providerId];
  const theme = PROVIDER_MASCOT_THEMES[providerId] ?? DEFAULT_MASCOT_THEME;
  const motionClass = motion ? ' island-mascot--motion' : '';
  const radius = Math.max(4, Math.round(size * 0.24));
  const style: MascotStyle = {
    width: size,
    height: size,
    borderRadius: radius,
    '--mascot-size': `${size}px`,
    '--mascot-spark-lg': `${Math.max(2, size * 0.11)}px`,
    '--mascot-spark-sm': `${Math.max(2, size * 0.08)}px`,
    '--mascot-orbit': `${size * 0.56}px`,
    '--mascot-alert-orbit-start': `${size * 0.44}px`,
    '--mascot-alert-orbit-end': `${size * 0.62}px`,
    '--mascot-accent': theme.accent,
    '--mascot-accent-soft': theme.accentSoft,
    '--mascot-accent-glow': theme.accentGlow,
    '--mascot-depth': theme.depth,
  };

  if (iconSrc) {
    return (
      <span className={`island-mascot island-mascot--${state}${motionClass}`} style={style}>
        <span className="island-mascot__aura" aria-hidden="true" />
        <span className="island-mascot__sprite" aria-hidden="true">
          <img
            src={iconSrc}
            alt=""
            className="island-mascot__image"
            draggable={false}
          />
        </span>
        <span className="island-mascot__spark island-mascot__spark--one" aria-hidden="true" />
        <span className="island-mascot__spark island-mascot__spark--two" aria-hidden="true" />
      </span>
    );
  }

  const label = providerId.charAt(0).toUpperCase();
  return (
    <span
      className={`island-mascot island-mascot-fallback island-mascot--fallback island-mascot--${state}${motionClass}`}
      style={style}
    >
      <span className="island-mascot__aura" aria-hidden="true" />
      <span className="island-mascot__sprite island-mascot__sprite--fallback" aria-hidden="true">
        <span className="island-mascot-fallback__letter" style={{ fontSize: size * 0.5 }}>
          {label}
        </span>
      </span>
      <span className="island-mascot__spark island-mascot__spark--one" aria-hidden="true" />
      <span className="island-mascot__spark island-mascot__spark--two" aria-hidden="true" />
    </span>
  );
}
