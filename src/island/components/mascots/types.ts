import type { IslandSessionPhase } from '../../../types';

export type MascotState = 'idle' | 'working' | 'alert';

export function phaseToMascotState(phase: IslandSessionPhase): MascotState {
  switch (phase) {
    case 'processing':
    case 'compacting':
      return 'working';
    case 'waitingforapproval':
    case 'waitingforanswer':
      return 'alert';
    default:
      return 'idle';
  }
}

export interface MascotProps {
  state: MascotState;
  size: number;
}
