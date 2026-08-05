package com.hippo.ehviewer.spider

import com.ehviewer.core.files.read
import com.ehviewer.core.files.write
import com.ehviewer.core.model.GalleryDetail
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.model.PowerStatus
import com.hippo.ehviewer.bridge.archive.ComicInfoTag
import com.hippo.ehviewer.bridge.archive.ComicInfoTagGroup
import com.hippo.ehviewer.bridge.archive.ComicInfoTagPower
import com.hippo.ehviewer.bridge.archive.GalleryComicInfoRequest
import com.hippo.ehviewer.bridge.archive.buildComicInfo
import com.hippo.ehviewer.bridge.archive.decodeComicInfoXml
import com.hippo.ehviewer.bridge.archive.encodeComicInfoXml
import com.hippo.ehviewer.client.EhUrl
import kotlinx.io.readByteArray
import kotlinx.serialization.Serializable
import okio.Path

const val COMIC_INFO_FILE = "ComicInfo.xml"

fun GalleryInfo.getComicInfo(): ComicInfo =
  buildComicInfo(
    GalleryComicInfoRequest(
      title = title,
      titleJpn = titleJpn,
      web = EhUrl.getGalleryDetailUrl(gid, token),
      rating = rating,
      pages = pages,
      simpleLanguage = simpleLanguage,
      simpleTags = simpleTags,
      tagGroups =
        (this as? GalleryDetail)?.tagGroups?.map { group ->
          ComicInfoTagGroup(
            namespace = group.namespace.value,
            tags =
              group.tags.map { tag ->
                ComicInfoTag(
                  text = tag.text,
                  power =
                    when (tag.power) {
                      PowerStatus.Solid -> ComicInfoTagPower.SOLID
                      PowerStatus.Active -> ComicInfoTagPower.ACTIVE
                      PowerStatus.Weak -> ComicInfoTagPower.WEAK
                    },
                )
              },
          )
        },
    )
  )

fun ComicInfo.toSimpleTags(): List<String>? = simpleTags

fun writeComicInfo(info: ComicInfo, file: Path) {
  val encoded = encodeComicInfoXml(info)
  file.write { write(encoded) }
}

fun readComicInfo(file: Path): ComicInfo? = runCatching {
  file.read { decodeComicInfoXml(readByteArray()) }
}
  .getOrNull()

@Serializable
data class ComicInfo(
  val series: String?,
  val alternateSeries: String?,
  val writer: List<String>?,
  val penciller: List<String>?,
  val genre: List<String>?,
  val web: String?,
  val pageCount: Int = 0,
  val languageISO: String?,
  val characters: List<String>?,
  val teams: List<String>?,
  val communityRating: String?,
  val simpleTags: List<String>? = null,
)
