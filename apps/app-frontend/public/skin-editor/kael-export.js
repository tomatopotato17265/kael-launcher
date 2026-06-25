window.kaelExportTexture = function () {
	if (typeof Texture === 'undefined' || !Texture.all || Texture.all.length === 0) {
		return null
	}

	var tex = Texture.all.find(function (t) {
		return t && t.name === 'kael_skin.png'
	}) || Texture.all[0]

	if (!tex) {
		return null
	}

	if (tex.canvas && typeof tex.canvas.toDataURL === 'function') {
		try {
			if (typeof tex.updateSource === 'function') {
				tex.updateSource(tex.canvas.toDataURL('image/png'))
			}
			return tex.canvas.toDataURL('image/png')
		} catch (err) {
			void err
		}
	}

	if (typeof tex.getDataURL === 'function') {
		try {
			return tex.getDataURL()
		} catch (err) {
			void err
		}
	}

	return tex.source || null
}
