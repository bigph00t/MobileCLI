import { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorInfo: null,
    };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ errorInfo });

    // Log to console in development
    if (import.meta.env.DEV) {
      console.error('ErrorBoundary caught an error:', error, errorInfo);
    }
  }

  handleReset = () => {
    this.setState({
      hasError: false,
      error: null,
      errorInfo: null,
    });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-[#1a1b26] flex items-center justify-center p-6">
          <div className="bg-[#24283b] border border-[#f7768e] rounded-xl shadow-xl max-w-lg w-full p-6">
            {/* Error Icon */}
            <div className="flex justify-center mb-4">
              <div className="w-16 h-16 rounded-full bg-[#f7768e]/20 flex items-center justify-center">
                <svg className="w-8 h-8 text-[#f7768e]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
              </div>
            </div>

            <h1 className="text-xl font-bold text-[#c0caf5] text-center mb-2">
              Something went wrong
            </h1>

            <p className="text-[#a9b1d6] text-center text-sm mb-4">
              The application encountered an unexpected error.
            </p>

            {/* Error Details (collapsed by default in production) */}
            {import.meta.env.DEV && this.state.error && (
              <div className="bg-[#1a1b26] rounded-lg p-4 mb-4 overflow-auto max-h-48">
                <p className="text-[#f7768e] font-mono text-sm mb-2">
                  {this.state.error.message}
                </p>
                {this.state.errorInfo && (
                  <pre className="text-[#565f89] font-mono text-xs whitespace-pre-wrap">
                    {this.state.errorInfo.componentStack}
                  </pre>
                )}
              </div>
            )}

            {/* Actions */}
            <div className="flex gap-3 justify-center">
              <button
                onClick={this.handleReset}
                className="px-4 py-2 bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] rounded-lg font-medium transition-colors"
              >
                Try Again
              </button>
              <button
                onClick={() => window.location.reload()}
                className="px-4 py-2 bg-[#414868] hover:bg-[#565f89] text-[#c0caf5] rounded-lg font-medium transition-colors"
              >
                Reload App
              </button>
            </div>

            {/* Help text */}
            <p className="text-[#565f89] text-xs text-center mt-6">
              If this keeps happening, please{' '}
              <a
                href="https://github.com/mobilecli/desktop/issues"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[#7aa2f7] hover:underline"
              >
                report the issue
              </a>
            </p>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary;
