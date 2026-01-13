import { useState } from 'react';
import { WelcomeStep } from './WelcomeStep';
import { ModeSelectionStep } from './ModeSelectionStep';
import { HostSetupStep } from './HostSetupStep';
import { ClientPairingStep } from './ClientPairingStep';
import { CompletionStep } from './CompletionStep';

type WizardStep = 'welcome' | 'mode' | 'host-setup' | 'client-pairing' | 'complete';

interface SetupWizardProps {
  onComplete: (mode: 'host' | 'client') => void;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  const [step, setStep] = useState<WizardStep>('welcome');
  const [mode, setMode] = useState<'host' | 'client'>('host');

  const handleModeSelect = (selectedMode: 'host' | 'client') => {
    setMode(selectedMode);
    setStep(selectedMode === 'host' ? 'host-setup' : 'client-pairing');
  };

  const handleBack = () => {
    switch (step) {
      case 'mode':
        setStep('welcome');
        break;
      case 'host-setup':
      case 'client-pairing':
        setStep('mode');
        break;
      case 'complete':
        setStep(mode === 'host' ? 'host-setup' : 'client-pairing');
        break;
    }
  };

  return (
    <div className="fixed inset-0 bg-gray-900 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg p-8 max-w-lg w-full mx-4 shadow-2xl">
        {/* Progress indicator */}
        <div className="flex justify-center mb-6">
          <div className="flex items-center gap-2">
            {['welcome', 'mode', mode === 'host' ? 'host-setup' : 'client-pairing', 'complete'].map((s, i) => (
              <div
                key={s}
                className={`w-2 h-2 rounded-full transition-colors ${
                  ['welcome', 'mode', mode === 'host' ? 'host-setup' : 'client-pairing', 'complete'].indexOf(step) >= i
                    ? 'bg-blue-500'
                    : 'bg-gray-600'
                }`}
              />
            ))}
          </div>
        </div>

        {step === 'welcome' && (
          <WelcomeStep onNext={() => setStep('mode')} />
        )}

        {step === 'mode' && (
          <ModeSelectionStep
            onSelect={handleModeSelect}
            onBack={handleBack}
          />
        )}

        {step === 'host-setup' && (
          <HostSetupStep
            onNext={() => setStep('complete')}
            onBack={handleBack}
          />
        )}

        {step === 'client-pairing' && (
          <ClientPairingStep
            onNext={() => setStep('complete')}
            onBack={handleBack}
          />
        )}

        {step === 'complete' && (
          <CompletionStep
            mode={mode}
            onComplete={() => onComplete(mode)}
            onBack={handleBack}
          />
        )}
      </div>
    </div>
  );
}
