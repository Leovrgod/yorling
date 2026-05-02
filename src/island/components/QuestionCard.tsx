import { useState } from 'react';
import type { IslandPendingQuestion } from '../../types';
import { useIslandActions } from '../hooks/useIslandState';
import { t } from '../i18n';

interface QuestionCardProps {
  question: IslandPendingQuestion;
  sessionId?: string;
}

/**
 * Question answer card with option buttons or free text input.
 */
export function QuestionCard({ question, sessionId }: QuestionCardProps) {
  const { answerQuestion, jumpToTerminal } = useIslandActions();
  const [loading, setLoading] = useState(false);
  const [customAnswer, setCustomAnswer] = useState('');
  const [expired, setExpired] = useState(false);
  const canAnswer = question.can_answer !== false;

  const handleAnswer = async (answer: string) => {
    if (expired) return;
    setLoading(true);
    try {
      await answerQuestion(question.request_id, answer);
    } catch {
      setExpired(true);
    }
    setLoading(false);
  };

  return (
    <div className={`island-question${expired ? ' island-question--expired' : ''}`} role="dialog" aria-label={t('collapsed.question_waiting')}>
      <div className="island-question__text" id={`question-${question.request_id}`}>{question.question}</div>

      {expired ? (
        <div className="island-permission__expired-notice">
          {t('permission.expired')}
        </div>
      ) : canAnswer ? (
        <>
          {question.options.length > 0 && (
            <div className="island-question__options" role="group" aria-labelledby={`question-${question.request_id}`}>
              {question.options.map((option, i) => (
                <button
                  key={i}
                  type="button"
                  className="island-question__option"
                  onClick={() => handleAnswer(option)}
                  disabled={loading}
                >
                  {option}
                </button>
              ))}
            </div>
          )}

          <div className="island-question__custom">
            <input
              type="text"
              className="island-question__input"
              placeholder={t('question.placeholder')}
              value={customAnswer}
              onChange={(e) => setCustomAnswer(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && customAnswer.trim()) {
                  handleAnswer(customAnswer.trim());
                }
              }}
              disabled={loading}
              aria-label={t('question.placeholder')}
            />
            <button
              type="button"
              className="island-question__send"
              onClick={() => {
                if (customAnswer.trim()) handleAnswer(customAnswer.trim());
              }}
              disabled={loading || !customAnswer.trim()}
            >
              {t('question.send')}
            </button>
          </div>
        </>
      ) : (
        <>
          {question.options.length > 0 && (
            <div className="island-question__options" role="group" aria-labelledby={`question-${question.request_id}`}>
              {question.options.map((option, i) => (
                <button
                  key={i}
                  type="button"
                  className="island-question__option"
                  disabled
                >
                  {option}
                </button>
              ))}
            </div>
          )}

          <div className="island-question__custom">
            <button
              type="button"
              className="island-question__send"
              onClick={() => {
                if (sessionId) void jumpToTerminal(sessionId);
              }}
              disabled={!sessionId}
            >
              {t('session.jump')}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
