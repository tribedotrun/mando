import React, { useState } from 'react';
import { useMemoryStore } from '#renderer/stores/memoryStore';
import { JournalTable } from '#renderer/components/JournalTable';
import { PatternsList } from '#renderer/components/PatternsList';

type SubTab = 'journal' | 'patterns';

const activeTabStyle: React.CSSProperties = {
  backgroundColor: 'var(--color-surface-3)',
  color: 'var(--color-text-1)',
};
const inactiveTabStyle: React.CSSProperties = { color: 'var(--color-text-4)' };

export function MemoryCard() {
  const [activeTab, setActiveTab] = useState<SubTab>('journal');
  const { patterns, distillerRunning, distillerResult, distillerError, runDistiller } =
    useMemoryStore();

  const pendingCount = patterns.filter((p) => p.status === 'pending').length;

  return (
    <div
      className="col-span-full rounded-lg border p-4"
      style={{
        borderColor: 'var(--color-border)',
        backgroundColor: 'var(--color-surface-1)',
      }}
    >
      {/* Header */}
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h3 className="text-sm font-semibold" style={{ color: 'var(--color-text-2)' }}>
            Memory
          </h3>
          <div className="flex gap-1">
            <button
              onClick={() => setActiveTab('journal')}
              className="rounded px-2 py-0.5 text-xs"
              style={activeTab === 'journal' ? activeTabStyle : inactiveTabStyle}
            >
              Journal
            </button>
            <button
              onClick={() => setActiveTab('patterns')}
              className="rounded px-2 py-0.5 text-xs"
              style={activeTab === 'patterns' ? activeTabStyle : inactiveTabStyle}
            >
              Patterns{pendingCount > 0 ? ` (${pendingCount})` : ''}
            </button>
          </div>
        </div>
        <button
          onClick={runDistiller}
          disabled={distillerRunning}
          className="rounded px-2 py-1 text-xs disabled:opacity-50"
          style={{
            backgroundColor: 'var(--color-surface-2)',
            color: 'var(--color-text-3)',
          }}
        >
          {distillerRunning ? 'Analyzing...' : 'Run Distiller'}
        </button>
      </div>

      {/* Distiller result / error */}
      {distillerError && (
        <div
          className="mb-3 rounded px-3 py-1.5 text-xs"
          style={{
            backgroundColor: 'var(--color-error-bg)',
            color: 'var(--color-error)',
          }}
        >
          {distillerError}
        </div>
      )}
      {distillerResult && (
        <div
          className="mb-3 rounded px-3 py-1.5 text-xs"
          style={{
            backgroundColor: 'var(--color-surface-2)',
            color: 'var(--color-text-3)',
          }}
        >
          {distillerResult}
        </div>
      )}

      {/* Sub-tab content */}
      {activeTab === 'journal' ? <JournalTable /> : <PatternsList />}
    </div>
  );
}
