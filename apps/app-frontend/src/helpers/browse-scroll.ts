const positions = new Map<string, number>()

export function saveBrowseScrollPosition(path: string) {
	const el = document.querySelector('.app-viewport')
	if (el) positions.set(path, el.scrollTop)
}

export function takeBrowseScrollPosition(path: string): number | undefined {
	const value = positions.get(path)
	positions.delete(path)
	return value
}

export function hasBrowseScrollPosition(path: string): boolean {
	return positions.has(path)
}
