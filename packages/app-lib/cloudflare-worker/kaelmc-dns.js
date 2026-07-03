// kaelmc-dns — Cloudflare Worker that manages <name>.kaelmc.net subdomains
// for Kael Launcher servers.
//
// Why this exists: the desktop app is distributed publicly, so it cannot
// embed a Cloudflare API token (anyone could extract it and edit the zone).
// Instead the token lives here as a Worker secret, and the app only gets two
// narrow operations: claim a subdomain and release it. Ownership of each
// subdomain is tracked in Workers KV per anonymous app install, with a TTL
// so names from abandoned installs free up on their own.
//
// Bindings required (see setup instructions):
//   secret  CF_API_TOKEN — API token scoped to Zone.DNS edit on the domain
//   var     ZONE_ID      — the zone id of the domain
//   var     DOMAIN       — e.g. "kaelmc.net" (must match the app's domain)
//   kv      OWNERS       — KV namespace mapping subdomain -> owner key

const SUBDOMAIN_RE = /^[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?$/;
const OWNER_RE = /^[a-zA-Z0-9-]{16,64}$/;
const IPV4_RE = /^\d{1,3}(\.\d{1,3}){3}$/;
// Ownership expires unless renewed; the app renews on every server start,
// so only long-abandoned installs lose their names
const OWNER_TTL_SECONDS = 60 * 60 * 24 * 90;

export default {
	async fetch(request, env) {
		if (request.method !== 'POST') {
			return json({ error: 'method not allowed' }, 405);
		}

		let body;
		try {
			body = await request.json();
		} catch {
			return json({ error: 'invalid json' }, 400);
		}

		const path = new URL(request.url).pathname;
		try {
			if (path === '/claim') return await claim(env, body);
			if (path === '/release') return await release(env, body);
		} catch (e) {
			return json({ error: `upstream failure: ${e.message}` }, 502);
		}
		return json({ error: 'not found' }, 404);
	},
};

async function claim(env, body) {
	const { subdomain, ip, port, owner } = body ?? {};
	if (typeof subdomain !== 'string' || !SUBDOMAIN_RE.test(subdomain)) {
		return json({ error: 'invalid subdomain' }, 400);
	}
	if (typeof ip !== 'string' || !IPV4_RE.test(ip)) {
		return json({ error: 'invalid ip' }, 400);
	}
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		return json({ error: 'invalid port' }, 400);
	}
	if (typeof owner !== 'string' || !OWNER_RE.test(owner)) {
		return json({ error: 'invalid owner' }, 400);
	}

	let candidate = subdomain;
	for (let attempt = 0; attempt < 3; attempt++) {
		const holder = await env.OWNERS.get(candidate);
		if (holder === null || holder === owner) {
			const fqdn = `${candidate}.${env.DOMAIN}`;
			await deleteRecords(env, fqdn);
			await createRecords(env, fqdn, ip, port);
			await env.OWNERS.put(candidate, owner, {
				expirationTtl: OWNER_TTL_SECONDS,
			});
			return json({ fqdn });
		}
		candidate = `${subdomain}-${randomSuffix()}`;
	}

	return json({ error: 'subdomain unavailable' }, 409);
}

async function release(env, body) {
	const { subdomain, owner } = body ?? {};
	if (typeof subdomain !== 'string' || !SUBDOMAIN_RE.test(subdomain)) {
		return json({ error: 'invalid subdomain' }, 400);
	}
	if (typeof owner !== 'string' || !OWNER_RE.test(owner)) {
		return json({ error: 'invalid owner' }, 400);
	}

	const holder = await env.OWNERS.get(subdomain);
	if (holder !== null && holder !== owner) {
		return json({ error: 'not the owner' }, 403);
	}

	await deleteRecords(env, `${subdomain}.${env.DOMAIN}`);
	await env.OWNERS.delete(subdomain);
	return json({ ok: true });
}

async function createRecords(env, fqdn, ip, port) {
	await cfApi(env, 'POST', `/zones/${env.ZONE_ID}/dns_records`, {
		type: 'A',
		name: fqdn,
		content: ip,
		proxied: false,
		ttl: 1,
	});

	try {
		await cfApi(env, 'POST', `/zones/${env.ZONE_ID}/dns_records`, {
			type: 'SRV',
			name: `_minecraft._tcp.${fqdn}`,
			data: { priority: 0, weight: 5, port, target: fqdn },
			ttl: 1,
		});
	} catch (e) {
		await deleteRecords(env, fqdn);
		throw e;
	}
}

async function deleteRecords(env, fqdn) {
	for (const name of [fqdn, `_minecraft._tcp.${fqdn}`]) {
		const records = await cfApi(
			env,
			'GET',
			`/zones/${env.ZONE_ID}/dns_records?name=${name}`,
		);
		for (const record of records) {
			await cfApi(
				env,
				'DELETE',
				`/zones/${env.ZONE_ID}/dns_records/${record.id}`,
			);
		}
	}
}

async function cfApi(env, method, path, body) {
	const response = await fetch(`https://api.cloudflare.com/client/v4${path}`, {
		method,
		headers: {
			authorization: `Bearer ${env.CF_API_TOKEN}`,
			'content-type': 'application/json',
		},
		body: body ? JSON.stringify(body) : undefined,
	});

	const data = await response.json();
	if (!data.success) {
		throw new Error(`Cloudflare API error: ${JSON.stringify(data.errors)}`);
	}
	return data.result;
}

function randomSuffix() {
	const bytes = new Uint8Array(4);
	crypto.getRandomValues(bytes);
	return Array.from(bytes, (b) => (b % 36).toString(36)).join('');
}

function json(data, status = 200) {
	return new Response(JSON.stringify(data), {
		status,
		headers: { 'content-type': 'application/json' },
	});
}
