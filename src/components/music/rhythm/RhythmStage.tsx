import { memo, useEffect, useMemo, useRef, useState } from 'react';
import {
  getLatestActiveSuccessPulseIdsByMidi,
  useRhythmScoreStore,
  type HitGrade,
  type SuccessPulse,
} from '../../../stores/rhythmScoreStore';
import { getCurrentSongTime, useRhythmGameStore } from '../../../stores/rhythmGameStore';
import type { TutorialSong } from '../../../data/tutorialSongTypes';
import { noteColorClass, noteInfoFromMidi, noteLabel } from '../../../data/musicKeyMapping';
import { prepareChart } from './engine/chart';
import { buildRhythmLaneGuides, RhythmRenderer } from './engine/renderer';

interface RhythmStageProps {
  song: TutorialSong;
  isDark: boolean;
}

function getLatestPulseGradesByMidi(pulses: readonly SuccessPulse[]): Map<number, HitGrade> {
  const now = performance.now();
  const latest = new Map<number, SuccessPulse>();
  for (const pulse of pulses) {
    if (now - pulse.createdAt > 360) continue;
    const previous = latest.get(pulse.midi);
    if (!previous || previous.createdAt < pulse.createdAt) {
      latest.set(pulse.midi, pulse);
    }
  }

  const result = new Map<number, HitGrade>();
  for (const [midi, pulse] of latest.entries()) {
    result.set(midi, pulse.grade);
  }
  return result;
}

export const RhythmStage = memo(function RhythmStage({
  song,
  isDark,
}: RhythmStageProps) {
  const chart = useMemo(() => prepareChart(song), [song]);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rendererRef = useRef<RhythmRenderer | null>(null);
  const [glError, setGlError] = useState<string | null>(null);
  const [stageWidth, setStageWidth] = useState(0);
  const laneGuides = useMemo(
    () => buildRhythmLaneGuides(chart.minMidi, chart.maxMidi, stageWidth, isDark),
    [chart.maxMidi, chart.minMidi, isDark, stageWidth],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;

    try {
      const renderer = new RhythmRenderer(canvas);
      rendererRef.current = renderer;
      setGlError(null);
      return () => {
        renderer.dispose();
        rendererRef.current = null;
      };
    } catch (error) {
      rendererRef.current = null;
      setGlError(error instanceof Error ? error.message : 'WebGL2 unavailable');
      return undefined;
    }
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    const renderer = rendererRef.current;
    if (!container || !renderer) return undefined;

    const resize = () => {
      const rect = container.getBoundingClientRect();
      renderer.resize(rect.width, rect.height);
      setStageWidth(rect.width);
    };

    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(container);
    window.addEventListener('resize', resize);

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', resize);
    };
  }, [glError]);

  useEffect(() => {
    rendererRef.current?.resetTransientState();
  }, [chart, isDark]);

  useEffect(() => {
    let frameId = 0;

    const tick = () => {
      const renderer = rendererRef.current;
      if (renderer) {
        const game = useRhythmGameStore.getState();
        const songTime = getCurrentSongTime(game);
        const liveScore = useRhythmScoreStore.getState();

        if (liveScore.enabled && game.isPlaying && !game.isDemoPlaying) {
          liveScore.sweepMisses(songTime, song.notes);
        }
        liveScore.cleanFeedbacks();

        const updatedScore = useRhythmScoreStore.getState();
        renderer.render({
          chart,
          songTime,
          judgedIndices: updatedScore.judgedIndices,
          activeHoldIndices: new Set(updatedScore.activeHolds.map((hold) => hold.noteIndex)),
          heldMidis: updatedScore.heldMidis,
          latestPulseIdsByMidi: getLatestActiveSuccessPulseIdsByMidi(updatedScore.successPulses),
          latestPulseGradesByMidi: getLatestPulseGradesByMidi(updatedScore.successPulses),
          combo: updatedScore.combo,
          isDark,
        });
      }

      frameId = window.requestAnimationFrame(tick);
    };

    frameId = window.requestAnimationFrame(tick);
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [chart, isDark, song.notes]);

  return (
    <div ref={containerRef} className="rhythm-stage" data-no-window-drag>
      {glError ? (
        <div className="rhythm-stage-fallback">
          <strong>WebGL2</strong>
          <span>{glError}</span>
        </div>
      ) : null}
      <canvas
        ref={canvasRef}
        className={`rhythm-stage-canvas ${glError ? 'is-hidden' : ''}`}
        aria-label="Rhythm stage"
      />
      <div className="rhythm-stage-label-strip" aria-hidden="true">
        {laneGuides.map((lane) => {
          const noteInfo = noteInfoFromMidi(lane.midi);

          return (
            <span
              key={lane.midi}
              className={`rhythm-stage-lane-label ${noteColorClass(noteInfo.note)} ${lane.black ? 'is-black' : ''}`}
              style={{ left: `${lane.center}px` }}
            >
              {noteLabel(noteInfo)}
            </span>
          );
        })}
      </div>
    </div>
  );
});
