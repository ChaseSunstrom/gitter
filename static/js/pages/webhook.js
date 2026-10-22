// One webhook: its settings, and its recent deliveries with what was sent and what came back.
import { html, get, send, toast, action, confirmDialog, timeEl, ago, $, $$ } from '../lib.js';
import { icon } from '../icons.js';
import { loadRepo, repoUrl, repoPage, mountRepoHeader, ciIcon } from '../repohead.js';
import { navigate } from '../main.js';

const EVENT_NAMES = { push: 'Pushes', issues: 'Issues', issue_comment: 'Comments', pull_request: 'Pull requests', ci_run: 'CI runs' };
export const eventsLabel = (evs) => (evs.includes('*') ? 'Everything' : evs.map((e) => EVENT_NAMES[e] || e).join(', '));

/** [icon, words] for a delivery or a hook's last one. */
export function deliveryState(d) {
  const code = d.response_code != null ? ` (${d.response_code})` : '';
  if (d.status === 'ok') return [ciIcon('success'), `Delivered${code}`];
  if (d.status === 'failed') return [ciIcon('failure'), `Failed${code}, gave up`];
  if (!d.attempts && !d.at) return [ciIcon('queued'), 'Waiting to send'];
  if (!d.tried_at && !d.at) return [ciIcon('running'), 'Sending'];
  return [ciIcon('failure'), `Failed${code}${d.next_at ? `, trying again ${ago(d.next_at)}` : ', trying again'}`];
}

const pretty = (s) => { try { return JSON.stringify(JSON.parse(s), null, 2); } catch { return s; } };

function row(d) {
  const [ic, words] = deliveryState(d);
  return html`<details class="ci-fold hook-delivery" data-did="${d.id}"><summary>${icon('chev-right', 'sm chev')}${ic}
    <b>${d.event}</b>${d.redelivery ? html` <span class="pill">Redelivery</span>` : ''}<span class="mono small muted" title="${d.guid}">${d.guid.slice(0, 8)}</span>
    <span class="grow small">${words}${d.error ? html` · <span class="c-red">${d.error}</span>` : ''}</span>
    <span class="small muted">${timeEl(d.tried_at || d.created_at)}${d.duration_ms != null ? ` · ${d.duration_ms} ms` : ''}</span></summary>
    <div class="box-body stack small" data-detail>Loading…</div></details>`;
}

function detail(d) {
  const headers = Object.entries(d.request_headers || {}).map(([k, v]) => `${k}: ${v}`).join('\n');
  return html`<div class="row"><b class="grow">Request</b><button class="btn sm" type="button" data-redeliver="${d.id}">Redeliver</button></div>
    ${d.attempts ? html`<pre class="setup-cmd">${headers}</pre>` : html`<p class="muted">Not sent yet.</p>`}
    <pre class="setup-cmd" style="max-height:360px">${pretty(d.payload)}</pre>
    <b>Response</b>
    ${d.response_code != null ? html`<pre class="setup-cmd" style="max-height:240px">${d.response_code}${d.response_body ? `\n\n${d.response_body}` : ''}</pre>`
      : html`<p class="muted">${d.error || 'No response yet.'}</p>`}`;
}

export default async function (ctx) {
  const meta = await loadRepo(ctx.params.owner, ctx.params.repo);
  if (!meta.can_write) throw Object.assign(new Error('Only people who can write to this repository can change its webhooks'), { status: 403 });
  const api = repoUrl(meta.owner, meta.name);
  const full = `${meta.owner}/${meta.name}`;
  const isNew = !!ctx.params.create;
  const d = await get(isNew ? `${api}/hooks` : `${api}/hooks/${encodeURIComponent(ctx.params.id)}`, { fresh: true });
  const hook = d.hook || { url: '', events: ['push'], active: true, allow_private: false };
  const all = hook.events.includes('*');
  const body = html`<div class="mid stack" style="max-width:820px">
    <a class="muted small ci-back" href="/${full}/settings">${icon('arrow-left', 'sm')} Settings</a>
    <form class="box" id="hook-form"><div class="box-head"><b>${isNew ? 'Add webhook' : 'Webhook'}</b></div><div class="box-body stack">
      <div class="field"><label for="h-url">Payload URL</label>
        <input id="h-url" class="input mono" type="url" required maxlength="2048" placeholder="https://example.com/hooks/gitter" value="${hook.url}">
        <span class="hint">Gitter sends a POST with a JSON body here. Redirects aren't followed.</span></div>
      <div class="field"><label for="h-secret">Secret</label>
        <input id="h-secret" class="input mono" type="password" autocomplete="new-password" maxlength="1024" ${isNew ? 'required' : ''} placeholder="${isNew ? 'A long random string' : 'Leave empty to keep the current secret'}">
        <span class="hint">Each delivery is signed with it in the X-Gitter-Signature-256 header, so your receiver can check it came from here.</span></div>
      <div class="field" role="group" aria-labelledby="h-ev-label"><span class="label" id="h-ev-label">What to send</span>
        <label class="check"><input type="radio" name="which" value="all" ${all ? 'checked' : ''}> Everything</label>
        <label class="check"><input type="radio" name="which" value="some" ${all ? '' : 'checked'}> Only these:</label>
        <div class="row wrap-ok" id="h-events" style="padding-left:24px">${d.events.map((e) => html`<label class="check small"><input type="checkbox" name="ev" value="${e}" ${!all && hook.events.includes(e) ? 'checked' : ''}> ${EVENT_NAMES[e] || e}</label>`)}</div>
      </div>
      <label class="check"><input type="checkbox" id="h-active" ${hook.active ? 'checked' : ''}><span>Active <span class="muted small">Nothing is sent while it's off.</span></span></label>
      <label class="check"><input type="checkbox" id="h-private" ${hook.allow_private ? 'checked' : ''} ${d.can_allow_private ? '' : 'disabled'}><span>Allow private addresses
        <span class="muted small">Lets it reach this server's own network, like localhost, 10.x and 192.168.x. Only site admins can change this.</span></span></label>
      <div class="row"><button class="btn primary" type="submit">${isNew ? 'Add webhook' : 'Save changes'}</button>
        ${isNew ? '' : html`<button class="btn" type="button" id="h-ping">Send a ping</button><span class="grow"></span><button class="btn danger" type="button" id="h-del">${icon('trash', 'sm')} Delete</button>`}</div>
    </div></form>
    ${isNew ? '' : html`<div class="box" id="deliveries"><div class="box-head"><b>Recent deliveries</b><span class="grow"></span><span class="muted small">The last 50. Failed ones are tried again after 1 minute, 5 minutes, 30 minutes, 2 hours and 6 hours.</span></div>
      ${d.deliveries.length ? html`<div>${d.deliveries.map(row)}</div>` : html`<div class="box-body muted small">Nothing sent yet.</div>`}</div>`}
  </div>`;
  return {
    title: `${isNew ? 'Add webhook' : 'Webhook'} · ${full}`,
    html: repoPage(meta, 'settings', body),
    mount(el) {
      mountRepoHeader(el, meta);
      const f = $('#hook-form', el);
      const which = () => $('input[name=which]:checked', f)?.value;
      const sync = () => { $('#h-events', el).hidden = which() === 'all'; };
      f.addEventListener('change', sync);
      sync();
      f.addEventListener('submit', action($('button[type=submit]', f), async () => {
        const events = which() === 'all' ? ['*'] : $$('input[name=ev]:checked', f).map((c) => c.value);
        if (!events.length) throw new Error('Pick at least one event, or Everything');
        const req = { url: $('#h-url', el).value.trim(), events, active: $('#h-active', el).checked };
        if ($('#h-secret', el).value) req.secret = $('#h-secret', el).value;
        if (d.can_allow_private) req.allow_private = $('#h-private', el).checked;
        if (isNew) {
          const r = await send('POST', `${api}/hooks`, req);
          toast('Webhook added. A ping is on its way.');
          navigate(`/${full}/settings/hooks/${r.id}`, { replace: true });
        } else {
          await send('PATCH', `${api}/hooks/${hook.id}`, req);
          toast('Webhook saved');
          navigate(location.pathname, { replace: true, scroll: false });
        }
      }));
      if (isNew) return;
      const ping = $('#h-ping', el);
      ping.addEventListener('click', action(ping, async () => {
        await send('POST', `${api}/hooks/${hook.id}/ping`);
        toast('Ping sent');
        navigate(location.pathname, { replace: true, scroll: false });
      }));
      const del = $('#h-del', el);
      del.addEventListener('click', action(del, async () => {
        if (!(await confirmDialog({ title: 'Delete this webhook?', text: 'Nothing more is sent to it, and its delivery log goes too.', ok: 'Delete', danger: true }))) return;
        await send('DELETE', `${api}/hooks/${hook.id}`);
        toast('Webhook deleted');
        navigate(`/${full}/settings`);
      }));
      const log = $('#deliveries', el);
      // `toggle` doesn't bubble, so listen in the capture phase.
      log.addEventListener('toggle', async (e) => {
        const det = e.target;
        if (!det.open || det.dataset.loaded) return;
        det.dataset.loaded = '1';
        const box = $('[data-detail]', det);
        try { box.innerHTML = detail(await get(`${api}/hooks/${hook.id}/deliveries/${det.dataset.did}`, { fresh: true })).s; }
        catch (x) { box.textContent = x.message; delete det.dataset.loaded; }
      }, true);
      log.addEventListener('click', (e) => {
        const b = e.target.closest('[data-redeliver]');
        if (!b) return;
        action(b, async () => {
          await send('POST', `${api}/hooks/${hook.id}/deliveries/${b.dataset.redeliver}/redeliver`);
          toast('Sent again');
          navigate(location.pathname, { replace: true, scroll: false });
        })(e);
      });
    },
  };
}
