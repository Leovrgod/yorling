import { useEffect, useRef, useState, type ElementType } from 'react';

interface MorphTextProps {
  text: string;
  className?: string;
  tag?: ElementType;
}

/**
 * Smooth blur-morph transition when text content changes.
 * Inspired by CodeIsland's MorphText: blur-out old text, swap, blur-in new text.
 */
export function MorphText({ text, className, tag: Tag = 'span' }: MorphTextProps) {
  const [displayText, setDisplayText] = useState(text);
  const [phase, setPhase] = useState<'idle' | 'blur-out' | 'blur-in'>('idle');
  const pendingRef = useRef(text);
  const displayTextRef = useRef(text);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    displayTextRef.current = displayText;
  }, [displayText]);

  useEffect(() => {
    const clearTimer = () => {
      if (timeoutRef.current !== null) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    };

    const runTransition = () => {
      clearTimer();
      setPhase('blur-out');

      timeoutRef.current = setTimeout(() => {
        const nextDisplayText = pendingRef.current;
        displayTextRef.current = nextDisplayText;
        setDisplayText(nextDisplayText);
        setPhase('blur-in');

        timeoutRef.current = setTimeout(() => {
          if (pendingRef.current !== displayTextRef.current) {
            runTransition();
            return;
          }

          setPhase('idle');
          timeoutRef.current = null;
        }, 40);
      }, 30);
    };

    if (text === displayTextRef.current) {
      if (phase !== 'idle') {
        setPhase('idle');
      }
      clearTimer();
      return;
    }

    pendingRef.current = text;

    if (phase === 'idle') {
      runTransition();
    }

    return () => {
      clearTimer();
    };
  }, [phase, text]);

  // Clean up on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current !== null) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  const morphClass = phase === 'idle'
    ? 'morph-text'
    : `morph-text morph-text--${phase}`;

  return (
    <Tag className={`${morphClass}${className ? ` ${className}` : ''}`}>
      {displayText}
    </Tag>
  );
}
