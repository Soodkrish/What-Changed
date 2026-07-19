import { useState } from "react";
import { X, HelpCircle, Copy, Check } from "lucide-react";

interface WebhookHelpModalProps {
  onClose: () => void;
}

function CopyableCode({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="relative group">
      <code className="block w-full px-3 py-2 bg-gray-900 text-green-400 text-xs rounded-lg font-mono break-all select-all pr-8">
        {code}
      </code>
      <button
        onClick={() => {
          navigator.clipboard.writeText(code);
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        }}
        className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-gray-400 hover:text-white rounded opacity-0 group-hover:opacity-100 transition-opacity"
      >
        {copied ? <Check className="w-3.5 h-3.5 text-green-400" /> : <Copy className="w-3.5 h-3.5" />}
      </button>
    </div>
  );
}

export function WebhookHelpModal({ onClose }: WebhookHelpModalProps) {
  const [activeTab, setActiveTab] = useState<"discord" | "telegram">("discord");

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onClick={onClose}>
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-full max-w-2xl max-h-[85vh] flex flex-col" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="flex items-center justify-between p-5 border-b border-gray-200">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-full bg-violet-100 flex items-center justify-center">
              <HelpCircle className="w-5 h-5 text-violet-600" />
            </div>
            <div>
              <h3 className="text-lg font-bold text-gray-900 dark:text-white">Webhook Setup Guide</h3>
              <p className="text-sm text-gray-500">Step-by-step instructions for each platform</p>
            </div>
          </div>
          <button onClick={onClose} className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg">
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-gray-200">
          <button
            onClick={() => setActiveTab("discord")}
            className={`flex-1 py-3 text-sm font-medium transition-colors ${
              activeTab === "discord"
                ? "text-indigo-600 border-b-2 border-indigo-600"
                : "text-gray-500 hover:text-gray-700"
            }`}
          >
            Discord
          </button>
          <button
            onClick={() => setActiveTab("telegram")}
            className={`flex-1 py-3 text-sm font-medium transition-colors ${
              activeTab === "telegram"
                ? "text-blue-600 border-b-2 border-blue-600"
                : "text-gray-500 hover:text-gray-700"
            }`}
          >
            Telegram
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-5">
          {activeTab === "discord" ? (
            <div className="space-y-5">
              {/* Discord Step 1 */}
              <Step number={1} title="Open your Discord server">
                <p className="text-sm text-gray-600">
                  You need <strong>Manage Webhooks</strong> permission in the server.
                </p>
              </Step>

              {/* Discord Step 2 */}
              <Step number={2} title="Create a Webhook">
                <ol className="text-sm text-gray-600 space-y-1.5 ml-1 list-decimal list-inside">
                  <li>Go to <strong>Server Settings → Integrations</strong></li>
                  <li>Click <strong>Webhooks</strong></li>
                  <li>Click <strong>New Webhook</strong></li>
                  <li>Give it a name (e.g. "What Changed Bot")</li>
                  <li><strong>Choose the channel</strong> where notifications should appear</li>
                  <li>Click <strong>Copy Webhook URL</strong></li>
                </ol>
              </Step>

              {/* Discord Step 3 */}
              <Step number={3} title="Paste the URL in What Changed?">
                <p className="text-sm text-gray-600 mb-2">
                  Go to <strong>Settings → Webhooks</strong>, click <strong>Add</strong>, and paste the URL:
                </p>
                <CopyableCode code="https://discord.com/api/webhooks/1234567890/AbCdEfGhIjKlMnOpQrStUvWxYz..." />
              </Step>

              {/* Discord Step 4 */}
              <Step number={4} title="Test it">
                <p className="text-sm text-gray-600">
                  Click the <strong>⚡ lightning icon</strong> next to your webhook to send a test ping.
                  You should see "🧪 What Changed? webhook test ping" in your Discord channel.
                </p>
              </Step>

              <div className="p-3 bg-indigo-50 rounded-lg text-xs text-indigo-700">
                <strong>Note:</strong> Discord has a 2000 character limit per message. If you have many changes, the notification shows the first few files and a "+N more" count.
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              {/* Telegram Step 1 */}
              <Step number={1} title="Create a Telegram Bot">
                <ol className="text-sm text-gray-600 space-y-1.5 ml-1 list-decimal list-inside">
                  <li>Open Telegram and search for <strong>@BotFather</strong></li>
                  <li>Send <code className="px-1.5 py-0.5 bg-gray-100 rounded text-xs font-mono">/newbot</code></li>
                  <li>Enter a <strong>name</strong> for your bot (e.g. "What Changed Bot")</li>
                  <li>Enter a <strong>username</strong> (must end in "bot", e.g. "what_changed_bot")</li>
                  <li>BotFather gives you a <strong>token</strong> — copy it</li>
                </ol>
              </Step>

              {/* Telegram Step 2 */}
              <Step number={2} title="Get your Chat ID">
                <p className="text-sm text-gray-600 mb-2">
                  Send <strong>any message</strong> to your new bot (like "hi"), then open this URL in your browser:
                </p>
                <CopyableCode code="https://api.telegram.org/botYOUR_TOKEN/getUpdates" />
                <p className="text-sm text-gray-600 mt-2">
                  Find <code className="px-1 py-0.5 bg-gray-100 rounded text-xs font-mono">{`"chat":{"id":123456789}`}</code> in the JSON — that number is your <strong>chat_id</strong>.
                </p>
                <p className="text-xs text-gray-400 mt-1">
                  For a group: add the bot as admin, send a message in the group, then check getUpdates. Groups have negative chat_ids.
                </p>
              </Step>

              {/* Telegram Step 3 */}
              <Step number={3} title="Paste the URL in What Changed?">
                <p className="text-sm text-gray-600 mb-2">
                  Go to <strong>Settings → Webhooks</strong>, click <strong>Add</strong>, and paste this URL:
                </p>
                <CopyableCode code="https://api.telegram.org/botYOUR_TOKEN/sendMessage?chat_id=YOUR_CHAT_ID" />
                <p className="text-xs text-gray-400 mt-1.5">
                  Replace <strong>YOUR_TOKEN</strong> with the token from BotFather and <strong>YOUR_CHAT_ID</strong> with the number from getUpdates.
                </p>
              </Step>

              {/* Telegram Step 4 */}
              <Step number={4} title="Test it">
                <p className="text-sm text-gray-600">
                  Click the <strong>⚡ lightning icon</strong> to send a test ping.
                  You should see "🧪 What Changed? webhook test ping" in your Telegram chat.
                </p>
              </Step>

              <div className="p-3 bg-blue-50 rounded-lg text-xs text-blue-700">
                <strong>Note:</strong> Telegram has a 4096 character limit per message. Longer notifications are automatically truncated.
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-gray-200 bg-gray-50 rounded-b-xl">
          <p className="text-xs text-gray-500">
            Both platforms work out of the box — no coding required. Just copy, paste, and test.
          </p>
        </div>
      </div>
    </div>
  );
}

function Step({ number, title, children }: { number: number; title: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3">
      <div className="flex-shrink-0 w-7 h-7 rounded-full bg-brand-100 text-brand-700 flex items-center justify-center text-sm font-bold">
        {number}
      </div>
      <div className="flex-1 min-w-0">
        <h4 className="text-sm font-semibold text-gray-900 mb-1.5">{title}</h4>
        {children}
      </div>
    </div>
  );
}
