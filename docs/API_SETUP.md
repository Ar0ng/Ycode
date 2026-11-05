# API configuration

Open **Settings → Models & API**. Each provider is configured on its own.

| Provider | Where to get a key | Default base URL | Protocol used |
|---|---|---|---|
| OpenAI | platform.openai.com → API keys | `https://api.openai.com/v1` | Responses API (`/responses`) |
| DeepSeek | platform.deepseek.com → API keys | `https://api.deepseek.com` | Chat Completions (`/chat/completions`) |
| Anthropic | platform.claude.com → API keys | `https://api.anthropic.com` | Messages API (`/v1/messages`) |

## Keys

- Paste the key and press **Save**. It is written to **Windows Credential Manager**
  (generic credential `Ycode` / `<provider>`), never to the database, settings
  file, logs or the UI. The UI can only ask *whether* a key exists.
- **Delete key** removes the credential. You can also remove it in Control Panel →
  Credential Manager → Windows Credentials.
- Keys are only sent to that provider's base URL.

## Base URL

Leave empty for the official endpoint. A custom URL must be `https://` (plain
`http://` is accepted only for `localhost`) and may not contain credentials or a
query string. Use it for a corporate gateway that speaks the same protocol — Ycode
does **not** pretend one vendor's protocol works for another.

## Test & refresh models

**Test & refresh models** calls the vendor's list-models endpoint. It costs no tokens
and verifies key + URL. The returned list is merged with Ycode's built-in capability
table (see `MODEL_RESEARCH.md`):

- Models your key can't see are greyed out ("not returned for your key").
- New models returned by the vendor appear automatically. For OpenAI (whose list has
  no capability data) a new model starts with **reasoning control disabled** until
  Ycode knows its supported levels; Claude and DeepSeek report effort levels themselves.
- You can type any model ID by hand in the model picker. Manual IDs are kept across
  refreshes.

## Reasoning effort

The slider under the input box shows only the levels the selected model accepts,
and the value displayed is exactly what is sent:

| Provider | Request field | Levels |
|---|---|---|
| OpenAI GPT-6.x | `reasoning.effort` | Astra/Sol: low–max · Terra/Luna: off, low–max |
| DeepSeek | `thinking` + `reasoning_effort` | off (thinking disabled), low, high, max |
| Claude 5.x | `output_config.effort` (+ adaptive thinking) | low, medium, high, xhigh, max |

Your last choice is remembered per model. Changing the level mid-chat on GPT-6 and
Claude 5.x models uses the vendors' in-band mechanisms (`configuration_update`;
per-message effort system message) so the prompt cache survives.

## Image generation (GPT Image)

Settings → General → **Image generation (GPT Image)** (on by default) and **Image model**
(`gpt-image-2.6-flare` default; `gpt-image-2.6-sunburst` is better at precise edits).

- Needs a saved **OpenAI** key, but works from any chat model: ask GPT, Claude or DeepSeek
  for a picture and it calls the `generate_image` / `edit_image` tools.
- Images are saved in `%APPDATA%\com.ycode.desktop\images\` and shown in the chat; click to
  view full size, hover for **Save as…**. Only the file name goes back into the conversation.
- To change an image, just ask ("make the background blue"); the model calls `edit_image`
  with the image's id. In Coding mode it can also edit PNG/JPEG/WebP files in the workspace.
- Billing is OpenAI's image-token pricing, separate from the chat usage line in Ycode.
- A blocked prompt shows "The model declined this request" (OpenAI `moderation_blocked`).

## Typical errors

| Message | Meaning |
|---|---|
| API key was rejected | 401 — wrong or revoked key |
| No remaining balance or quota | 402 / `insufficient_quota` |
| Model doesn't exist / no access | 404, or 403 for models your key can't use |
| Rate limit reached | 429 — Ycode retries twice with back-off before showing this |
| Provider overloaded | 503 / 529 — retried automatically |
| Stream interrupted | connection dropped mid-answer; partial text is kept, press **Retry** |
