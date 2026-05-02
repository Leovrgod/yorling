export type KeyboardTextKind = 'display' | 'description';
export type KeyboardTextDensity = 'regular' | 'balanced' | 'compact';

const CJK_CHARACTER = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;
const NARROW_CHARACTER = /[ilI1.,'"`|]/u;
const WIDE_LATIN_CHARACTER = /[MWQ@#%&]/u;
const WHITESPACE_CHARACTER = /\s/u;

export function estimateVisualLength(text: string): number {
  const trimmedText = text.trim();

  if (!trimmedText) {
    return 0;
  }

  let total = 0;

  for (const character of trimmedText) {
    if (WHITESPACE_CHARACTER.test(character)) {
      total += 0.38;
      continue;
    }

    if (CJK_CHARACTER.test(character)) {
      total += 1.12;
      continue;
    }

    if (NARROW_CHARACTER.test(character)) {
      total += 0.56;
      continue;
    }

    if (WIDE_LATIN_CHARACTER.test(character)) {
      total += 0.96;
      continue;
    }

    total += 0.82;
  }

  return Number(total.toFixed(2));
}

export function getKeyboardTextDensity(
  text: string,
  kind: KeyboardTextKind,
): KeyboardTextDensity {
  const visualLength = estimateVisualLength(text);

  if (kind === 'display') {
    if (visualLength >= 6.4) {
      return 'compact';
    }

    if (visualLength >= 4) {
      return 'balanced';
    }

    return 'regular';
  }

  if (visualLength >= 11.4) {
    return 'compact';
  }

  if (visualLength >= 7.6) {
    return 'balanced';
  }

  return 'regular';
}

export function getKeyboardTextClasses(displayText: string, descriptionText: string): {
  displayClassName: string;
  descriptionClassName: string;
} {
  const displayDensity = getKeyboardTextDensity(displayText, 'display');
  const descriptionDensity = getKeyboardTextDensity(descriptionText, 'description');

  return {
    displayClassName: `keyboard-key-display${displayDensity === 'regular' ? '' : ` ${displayDensity}`}`,
    descriptionClassName: `keyboard-key-description${descriptionDensity === 'regular' ? '' : ` ${descriptionDensity}`}`,
  };
}
