/**
 * CurseForge helpers.
 *
 * All CurseForge traffic goes through the Rust backend (the `curseforge` Tauri
 * plugin) so the API key stays embedded in the compiled binary and is never
 * exposed to the webview. These are thin wrappers over those commands.
 */
import { invoke } from '@tauri-apps/api/core'

/** CurseForge content class IDs for Minecraft. */
export const CurseForgeClassId = {
	Mods: 6,
	Modpacks: 4471,
	ResourcePacks: 12,
	Worlds: 17,
	Shaders: 6552,
	BukkitPlugins: 5,
	DataPacks: 6945,
	Customization: 4546,
} as const

/** CurseForge mod loader type enum. */
export const CurseForgeModLoaderType = {
	Any: 0,
	Forge: 1,
	Fabric: 4,
	Quilt: 5,
	NeoForge: 6,
} as const

/** CurseForge search `sortField` values. */
export const CurseForgeSortField = {
	Featured: 1,
	Popularity: 2,
	LastUpdated: 3,
	Name: 4,
	TotalDownloads: 6,
} as const

export interface CurseForgePagination {
	index: number
	pageSize: number
	resultCount: number
	totalCount: number
}

export interface CurseForgeModAsset {
	thumbnailUrl?: string | null
	url?: string | null
}

export interface CurseForgeModAuthor {
	id: number
	name: string
	url?: string | null
}

export interface CurseForgeCategory {
	id: number
	name: string
	slug?: string | null
	classId?: number | null
	isClass?: boolean | null
	iconUrl?: string | null
}

export interface CurseForgeModLinks {
	websiteUrl?: string | null
	wikiUrl?: string | null
	issuesUrl?: string | null
	sourceUrl?: string | null
}

export interface CurseForgeFileHash {
	value: string
	/** 1 = Sha1, 2 = Md5 */
	algo: number
}

export interface CurseForgeFile {
	id: number
	modId: number
	displayName: string
	fileName: string
	fileDate?: string | null
	downloadUrl?: string | null
	downloadCount?: number | null
	fileLength?: number | null
	releaseType?: number | null
	isAvailable: boolean
	gameVersions: string[]
	hashes: CurseForgeFileHash[]
}

export interface CurseForgeModScreenshot {
	id: number
	title?: string | null
	description?: string | null
	thumbnailUrl?: string | null
	url?: string | null
}

export interface CurseForgeMod {
	id: number
	gameId: number
	name: string
	slug: string
	summary: string
	downloadCount: number
	classId?: number | null
	authors: CurseForgeModAuthor[]
	logo?: CurseForgeModAsset | null
	categories: CurseForgeCategory[]
	links?: CurseForgeModLinks | null
	latestFiles: CurseForgeFile[]
	screenshots?: CurseForgeModScreenshot[]
	allowModDistribution?: boolean | null
	dateCreated?: string | null
	dateModified?: string | null
}

export interface CurseForgeSearchResponse {
	data: CurseForgeMod[]
	pagination: CurseForgePagination
}

export interface CurseForgeMinecraftGameVersion {
	id: number
	versionString: string
}

export interface CurseForgeSearchParams {
	classId?: number
	categoryId?: number
	gameVersion?: string
	searchFilter?: string
	sortField?: number
	sortOrder?: 'asc' | 'desc'
	modLoaderType?: number
	index?: number
	pageSize?: number
}

/** Whether CurseForge integration is available in this build. */
export async function curseforge_is_enabled(): Promise<boolean> {
	return await invoke('plugin:curseforge|curseforge_is_enabled')
}

export async function curseforge_search(
	params: CurseForgeSearchParams,
): Promise<CurseForgeSearchResponse> {
	return await invoke('plugin:curseforge|curseforge_search', { params })
}

export async function curseforge_get_mod(modId: number): Promise<CurseForgeMod> {
	return await invoke('plugin:curseforge|curseforge_get_mod', { modId })
}

export async function curseforge_get_description(modId: number): Promise<string> {
	return await invoke('plugin:curseforge|curseforge_get_description', { modId })
}

export async function curseforge_get_files(modId: number): Promise<CurseForgeFile[]> {
	return await invoke('plugin:curseforge|curseforge_get_files', { modId })
}

export async function curseforge_categories(
	classId?: number,
): Promise<CurseForgeCategory[]> {
	return await invoke('plugin:curseforge|curseforge_categories', {
		classId: classId ?? null,
	})
}

export async function curseforge_minecraft_versions(): Promise<
	CurseForgeMinecraftGameVersion[]
> {
	return await invoke('plugin:curseforge|curseforge_minecraft_versions')
}

/**
 * Install a single CurseForge file (mod/resource pack/shader/data pack) into
 * a profile. Returns the profile-relative path of the installed project.
 */
export async function curseforge_install_file(
	profilePath: string,
	modId: number,
	fileId: number,
	projectType?: string,
): Promise<string> {
	return await invoke('plugin:curseforge|curseforge_install_file', {
		profilePath,
		modId,
		fileId,
		projectType: projectType ?? null,
	})
}

/**
 * Install a CurseForge modpack (by mod + file ID) as a brand new profile.
 * Returns the profile-relative path of the created profile.
 */
export async function curseforge_install_modpack(
	modId: number,
	fileId: number,
): Promise<string> {
	return await invoke('plugin:curseforge|curseforge_install_modpack', {
		modId,
		fileId,
	})
}

/**
 * Map an app project-type string to the matching CurseForge class ID.
 */
export function projectTypeToCurseForgeClassId(projectType: string): number {
	switch (projectType) {
		case 'modpack':
			return CurseForgeClassId.Modpacks
		case 'resourcepack':
			return CurseForgeClassId.ResourcePacks
		case 'shader':
			return CurseForgeClassId.Shaders
		case 'datapack':
			return CurseForgeClassId.DataPacks
		case 'world':
			return CurseForgeClassId.Worlds
		case 'plugin':
			return CurseForgeClassId.BukkitPlugins
		case 'mod':
		default:
			return CurseForgeClassId.Mods
	}
}

/**
 * Map a CurseForge class ID back to the matching app project-type string.
 */
export function curseForgeClassIdToProjectType(classId?: number | null): string {
	switch (classId) {
		case CurseForgeClassId.Modpacks:
			return 'modpack'
		case CurseForgeClassId.ResourcePacks:
			return 'resourcepack'
		case CurseForgeClassId.Shaders:
			return 'shader'
		case CurseForgeClassId.DataPacks:
			return 'datapack'
		case CurseForgeClassId.Worlds:
			return 'world'
		case CurseForgeClassId.BukkitPlugins:
			return 'plugin'
		case CurseForgeClassId.Mods:
		default:
			return 'mod'
	}
}

/**
 * Map an app loader name to the matching CurseForge modLoaderType, or undefined
 * if it has no CurseForge equivalent.
 */
export function loaderToCurseForgeModLoaderType(loader: string): number | undefined {
	switch (loader.toLowerCase()) {
		case 'forge':
			return CurseForgeModLoaderType.Forge
		case 'fabric':
			return CurseForgeModLoaderType.Fabric
		case 'quilt':
			return CurseForgeModLoaderType.Quilt
		case 'neoforge':
			return CurseForgeModLoaderType.NeoForge
		default:
			return undefined
	}
}

/** Map a CurseForge file `releaseType` (1/2/3) to a Modrinth release channel. */
export function curseForgeReleaseTypeToChannel(releaseType?: number | null): string {
	switch (releaseType) {
		case 2:
			return 'beta'
		case 3:
			return 'alpha'
		default:
			return 'release'
	}
}

/**
 * Choose the best CurseForge file to install into an instance, preferring files
 * that match the instance's game version and then its loader, newest first.
 */
export function pickCurseForgeFileForInstance(
	files: CurseForgeFile[],
	target: { gameVersion?: string | null; loader?: string | null },
): CurseForgeFile | undefined {
	const available = files.filter((f) => f.isAvailable !== false)
	const byVersion = available.filter(
		(f) => !target.gameVersion || f.gameVersions.includes(target.gameVersion),
	)
	const byLoader = byVersion.filter(
		(f) =>
			!target.loader ||
			loaderToCurseForgeModLoaderType(target.loader) === undefined ||
			f.gameVersions.some((v) => v.toLowerCase() === target.loader!.toLowerCase()),
	)
	const pool = byLoader.length ? byLoader : byVersion.length ? byVersion : available
	return [...pool].sort((a, b) => (b.fileDate ?? '').localeCompare(a.fileDate ?? ''))[0]
}

/**
 * CurseForge lists loader names and Minecraft versions together in a file's
 * `gameVersions`. Split them into Modrinth-style `loaders` and `game_versions`
 * using the known loader names.
 */
function splitCurseForgeGameVersions(
	entries: string[],
	loaderNames: Set<string>,
): { gameVersions: string[]; loaders: string[] } {
	const gameVersions: string[] = []
	const loaders: string[] = []
	for (const entry of entries) {
		const lower = entry.toLowerCase()
		if (loaderNames.has(lower)) {
			loaders.push(lower)
		} else if (/^\d/.test(entry)) {
			gameVersions.push(entry)
		}
	}
	return { gameVersions, loaders }
}

/** Map CurseForge files into Modrinth-shaped version objects for the version list. */
export function mapCurseForgeFilesToVersions(
	mod: CurseForgeMod,
	files: CurseForgeFile[],
	loaderNames: Set<string>,
) {
	return files.map((file) => {
		const { gameVersions, loaders } = splitCurseForgeGameVersions(
			file.gameVersions ?? [],
			loaderNames,
		)
		return {
			id: String(file.id),
			project_id: `cf-${mod.id}`,
			name: file.displayName,
			version_number: file.displayName,
			version_type: curseForgeReleaseTypeToChannel(file.releaseType),
			game_versions: gameVersions,
			loaders,
			date_published: file.fileDate ?? new Date().toISOString(),
			downloads: Math.round(file.downloadCount ?? 0),
			files: [
				{
					filename: file.fileName,
					size: file.fileLength ?? 0,
					primary: true,
					url: file.downloadUrl ?? '',
				},
			],
		}
	})
}

/**
 * Map a CurseForge mod into the Modrinth-v2 project shape consumed by the
 * in-app project page (header, description, gallery). The raw CurseForge mod and
 * files are attached under `curseforge` so the install action can use them.
 */
export function mapCurseForgeModToProject(
	mod: CurseForgeMod,
	options: {
		projectType: string
		description?: string
		files: CurseForgeFile[]
	},
) {
	const { projectType, description, files } = options
	const categories = mod.categories.map((c) => c.name)
	const gallery = (mod.screenshots ?? [])
		.filter((s) => s.url)
		.map((s) => ({
			url: s.url as string,
			raw_url: s.url as string,
			title: s.title ?? '',
			description: s.description ?? '',
			created: mod.dateModified ?? mod.dateCreated ?? new Date().toISOString(),
			featured: false,
		}))
	return {
		id: `cf-${mod.id}`,
		slug: mod.slug,
		project_type: projectType,
		title: mod.name,
		description: mod.summary,
		body: description || mod.summary,
		icon_url: mod.logo?.url ?? mod.logo?.thumbnailUrl ?? '',
		downloads: Math.round(mod.downloadCount ?? 0),
		followers: 0,
		categories,
		display_categories: categories,
		status: 'approved',
		gallery,
		featured_gallery: null,
		color: null,
		client_side: 'optional',
		server_side: 'optional',
		license: '',
		team: '',
		organization: null,
		date_created: mod.dateCreated ?? '',
		date_modified: mod.dateModified ?? '',
		versions: files.map((f) => String(f.id)),
		loaders: [],
		game_versions: [],
		website_url: mod.links?.websiteUrl ?? `https://www.curseforge.com/minecraft/${mod.slug}`,
		curseforge: { mod, files },
	}
}
