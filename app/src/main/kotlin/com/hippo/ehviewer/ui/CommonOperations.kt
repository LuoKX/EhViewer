/*
 * Copyright 2016 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.ui

import android.Manifest
import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.DriveFileMove
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.FavoriteBorder
import androidx.compose.material.icons.filled.HeartBroken
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.files.delete
import com.ehviewer.core.files.exists
import com.ehviewer.core.files.isDirectory
import com.ehviewer.core.files.write
import com.ehviewer.core.i18n.R
import com.ehviewer.core.model.BaseGalleryInfo
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.model.GalleryInfo.Companion.NOT_FAVORITED
import com.ehviewer.core.ui.component.LabeledCheckbox
import com.ehviewer.core.util.isAtLeastT
import com.ehviewer.core.util.mapToLongArray
import com.ehviewer.core.util.toEpochMillis
import com.ehviewer.core.util.toLocalDateTime
import com.ehviewer.core.util.withIOContext
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.download.downloadEntryChoice
import com.hippo.ehviewer.bridge.download.favoriteEntryChoice
import com.hippo.ehviewer.bridge.download.partitionDownloadEntries
import com.hippo.ehviewer.bridge.download.selectedDownloadLabel
import com.hippo.ehviewer.bridge.download.selectedFavoriteSlot
import com.hippo.ehviewer.client.EhEngine
import com.hippo.ehviewer.client.EhUtils
import com.hippo.ehviewer.download.DownloadManager
import com.hippo.ehviewer.download.DownloadService
import com.hippo.ehviewer.download.downloadDir
import com.hippo.ehviewer.download.downloadLocation
import com.hippo.ehviewer.download.tempDownloadDir
import com.hippo.ehviewer.ui.destinations.ReaderScreenDestination
import com.hippo.ehviewer.ui.reader.ReaderScreenArgs
import com.hippo.ehviewer.ui.tools.DialogState
import com.hippo.ehviewer.ui.tools.awaitConfirmationOrCancel
import com.hippo.ehviewer.ui.tools.awaitResult
import com.hippo.ehviewer.ui.tools.awaitSelectDate
import com.hippo.ehviewer.ui.tools.awaitSelectItem
import com.hippo.ehviewer.ui.tools.awaitSelectItemWithCheckBox
import com.hippo.ehviewer.ui.tools.awaitSelectItemWithIcon
import com.hippo.ehviewer.ui.tools.awaitSelectItemWithIconAndTextField
import com.hippo.ehviewer.util.FavouriteStatusRouter
import com.hippo.ehviewer.util.bgWork
import com.hippo.ehviewer.util.requestPermission
import com.hippo.ehviewer.util.restartApplication
import com.ramcosta.composedestinations.navigation.DestinationsNavigator
import kotlin.time.Clock
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.datetime.DateTimeUnit
import kotlinx.datetime.LocalDate
import kotlinx.datetime.TimeZone
import kotlinx.datetime.minus
import kotlinx.datetime.todayIn
import moe.tarsin.coroutines.runSuspendCatching
import moe.tarsin.string
import moe.tarsin.tip
import okio.Path
import splitties.init.appCtx

private fun removeNoMediaFile(downloadDir: Path) {
  (downloadDir / ".nomedia").delete()
}

private fun ensureNoMediaFile(downloadDir: Path) {
  (downloadDir / ".nomedia").apply { if (!exists()) write {} }
}

private val lck = Mutex()

suspend fun keepNoMediaFileStatus(
  downloadDir: Path = downloadLocation,
  mediaScan: Boolean = Settings.mediaScan.value,
) {
  if (downloadDir.isDirectory) {
    lck.withLock {
      if (mediaScan) {
        removeNoMediaFile(downloadDir)
      } else {
        ensureNoMediaFile(downloadDir)
      }
    }
  }
}

fun getFavoriteIcon(favorited: Boolean) =
  if (favorited) Icons.Default.Favorite else Icons.Default.FavoriteBorder

context(_: DialogState, _: MainActivity)
suspend fun startDownload(forceDefault: Boolean, vararg galleryInfos: BaseGalleryInfo) {
  if (isAtLeastT) {
    requestPermission(Manifest.permission.POST_NOTIFICATIONS)
  }
  val (existing, added) =
    partitionDownloadEntries(
      BooleanArray(galleryInfos.size) { DownloadManager.containDownloadInfo(galleryInfos[it].gid) }
    )
  val toStart = existing.map { galleryInfos[it] }
  val toAdd = added.map { galleryInfos[it] }
  if (toStart.isNotEmpty()) {
    val list = toStart.mapToLongArray(GalleryInfo::gid)
    DownloadService.startRangeDownload(list)
  }
  if (toAdd.isEmpty()) {
    return tip(R.string.added_to_download_list)
  }
  val hasDefault = !forceDefault && Settings.hasDefaultDownloadLabel
  val defaultLabel = if (hasDefault) Settings.defaultDownloadLabel else null
  val choice =
    downloadEntryChoice(
      forceDefault,
      hasDefault,
      defaultLabel == null,
      defaultLabel != null && DownloadManager.containLabel(defaultLabel),
      DownloadManager.labelList.isEmpty(),
    )
  val label = defaultLabel.takeIf { choice.useDefault }
  if (choice.direct) {
    // Got default label
    for (gi in toAdd) {
      DownloadService.startDownload(gi, label)
    }
    // Notify
    tip(R.string.added_to_download_list)
  } else {
    // Let use chose label
    val list = DownloadManager.labelList
    val items = buildList {
      add(string(R.string.default_download_label_name))
      list.forEach {
        add(it.label)
      }
    }
    val (selected, checked) =
      awaitSelectItemWithCheckBox(
        items,
        title = R.string.download,
        checkBoxText = R.string.remember_download_label,
      )
    val label1 = selectedDownloadLabel(items, selected)?.takeIf { DownloadManager.containLabel(it) }
    // Start download
    for (gi in toAdd) {
      DownloadService.startDownload(gi, label1)
    }
    // Save settings
    if (checked) {
      Settings.hasDefaultDownloadLabel = true
      Settings.defaultDownloadLabel = label1
    } else {
      Settings.hasDefaultDownloadLabel = false
    }
    tip(R.string.added_to_download_list)
  }
}

context(_: DialogState)
suspend fun modifyFavorites(galleryInfo: GalleryInfo): Boolean {
  val localFavorited = EhDB.containLocalFavorites(galleryInfo.gid)
  val signedIn = Settings.hasSignedIn.value
  val choice =
    favoriteEntryChoice(
      signedIn,
      galleryInfo.favoriteSlot,
      if (signedIn) Settings.defaultFavSlot.value else -2,
      galleryInfo.favoriteNote == null,
    )
  if (choice.chooser) {
    val localFav = getFavoriteIcon(localFavorited) to appCtx.getString(R.string.local_favorites)
    val cloudFav =
      Settings.favCat.mapIndexed { index, name ->
        getFavoriteIcon(galleryInfo.favoriteSlot == index) to name
      }
    val items = buildList {
      if (choice.removeRow) {
        val remove = Icons.Default.HeartBroken to appCtx.getString(R.string.remove_from_favourites)
        add(remove)
      }
      add(localFav)
      addAll(cloudFav)
    }
    if (choice.fetchNote) {
      galleryInfo.favoriteNote = bgWork {
        EhEngine.getFavoriteNote(galleryInfo.gid, galleryInfo.token)
      }
    }
    val (slot, note) =
      awaitSelectItemWithIconAndTextField(
        items,
        title = R.string.add_favorites_dialog_title,
        hint = R.string.favorite_note,
        initialNote = galleryInfo.favoriteNote.orEmpty(),
        maxChar = MAX_FAVNOTE_CHAR,
      )
    return doModifyFavorites(
      galleryInfo,
      selectedFavoriteSlot(choice.removeRow, slot),
      localFavorited,
      note,
    )
  }
  return doModifyFavorites(galleryInfo, choice.slot, localFavorited)
}

private suspend fun doModifyFavorites(
  galleryInfo: GalleryInfo,
  slot: Int = NOT_FAVORITED,
  localFavorited: Boolean = true,
  note: String = "",
) =
  with(galleryInfo) {
    val request =
      com.hippo.ehviewer.bridge.LocalFavoriteMutationRequest(
        version = com.hippo.ehviewer.bridge.LIBRARY_RULE_API_VERSION,
        requestedSlot = slot,
        currentSlot = favoriteSlot,
        localFavorited = localFavorited,
      )
    val plan = com.hippo.ehviewer.bridge.resolveLocalFavoriteMutation(request)
    when (plan.action) {
      com.hippo.ehviewer.bridge.LocalFavoriteAction.REMOVE_CLOUD -> {
        EhEngine.modifyFavorites(gid, token)
        favoriteName = null
        favoriteNote = null
      }
      com.hippo.ehviewer.bridge.LocalFavoriteAction.REMOVE_LOCAL ->
        EhDB.removeLocalFavorites(galleryInfo)
      com.hippo.ehviewer.bridge.LocalFavoriteAction.ADD_LOCAL -> EhDB.putLocalFavorites(galleryInfo)
      com.hippo.ehviewer.bridge.LocalFavoriteAction.SET_CLOUD -> {
        val cloudSlot = requireNotNull(plan.cloudSlot)
        EhEngine.modifyFavorites(gid, token, cloudSlot, note)
        favoriteName = Settings.favCat[cloudSlot]
        favoriteNote = note
      }
    }
    favoriteSlot = plan.nextFavoriteSlot
    FavouriteStatusRouter.notify(galleryInfo)
    plan.added
  }

suspend fun removeFromFavorites(galleryInfo: GalleryInfo) =
  doModifyFavorites(
    galleryInfo = galleryInfo,
    localFavorited = EhDB.containLocalFavorites(galleryInfo.gid),
  )

context(_: DestinationsNavigator)
fun navToReader(info: BaseGalleryInfo, page: Int = -1) =
  navToReader(ReaderScreenArgs.Gallery(info, page))

context(_: DestinationsNavigator)
fun navToReader(path: String) = navToReader(ReaderScreenArgs.Archive(path))

context(nav: DestinationsNavigator)
private fun navToReader(args: ReaderScreenArgs) =
  nav.navigate(ReaderScreenDestination(args)) { launchSingleTop = true }

context(_: DialogState, _: MainActivity, _: DestinationsNavigator)
suspend fun doGalleryInfoAction(info: BaseGalleryInfo) {
  val downloaded = DownloadManager.getDownloadState(info.gid) != DownloadInfo.STATE_INVALID
  val favorited = info.favoriteSlot != NOT_FAVORITED
  val items = buildList {
    add(Icons.AutoMirrored.Default.MenuBook to R.string.read)
    val download =
      if (downloaded) {
        Icons.Default.Delete to R.string.delete_downloads
      } else {
        Icons.Default.Download to R.string.download
      }
    add(download)
    val favorite =
      if (favorited) {
        Icons.Default.HeartBroken to R.string.remove_from_favourites
      } else {
        Icons.Default.Favorite to R.string.add_to_favourites
      }
    add(favorite)
    if (downloaded) {
      add(Icons.AutoMirrored.Default.DriveFileMove to R.string.download_move_dialog_title)
    }
  }
  val selected = awaitSelectItemWithIcon(items, EhUtils.getSuitableTitle(info))
  when (selected) {
    0 -> {
      EhDB.putHistoryInfo(info)
      navToReader(info)
    }
    1 ->
      if (downloaded) {
        confirmRemoveDownload(info)
      } else {
        startDownload(false, info)
      }
    2 ->
      if (favorited) {
        runSuspendCatching {
          removeFromFavorites(info)
          tip(R.string.remove_from_favorite_success)
        }
          .onFailure {
            tip(R.string.remove_from_favorite_failure)
          }
      } else {
        runSuspendCatching {
          modifyFavorites(info)
          tip(R.string.add_to_favorite_success)
        }
          .onFailure {
            tip(R.string.add_to_favorite_failure)
          }
      }
    3 -> showMoveDownloadLabel(info)
  }
}

private const val MAX_FAVNOTE_CHAR = 200

context(_: DialogState)
private suspend fun confirmRemoveDownload(text: String): Boolean {
  val checked =
    awaitResult(
      initial = Settings.removeImageFiles,
      title = R.string.download_remove_dialog_title,
    ) {
      Column {
        Text(
          text = text,
          style = MaterialTheme.typography.titleMedium,
        )
        Spacer(modifier = Modifier.height(16.dp))
        LabeledCheckbox(
          modifier = Modifier.fillMaxWidth(),
          checked = expectedValue,
          onCheckedChange = { expectedValue = it },
          label = stringResource(id = R.string.download_remove_dialog_check_text),
          indication = null,
        )
      }
    }
  Settings.removeImageFiles = checked
  return checked
}

context(_: DialogState)
suspend fun confirmRemoveDownload(info: GalleryInfo) {
  val text =
    appCtx.getString(R.string.download_remove_dialog_message, EhUtils.getSuitableTitle(info))
  val checked = confirmRemoveDownload(text)
  withIOContext {
    DownloadManager.deleteDownload(info.gid, checked)
  }
}

context(_: DialogState)
suspend fun confirmRemoveDownloadRange(list: Collection<DownloadInfo>) {
  val text = appCtx.getString(R.string.download_remove_dialog_message_2, list.size)
  val checked = confirmRemoveDownload(text)
  withIOContext {
    // Delete
    DownloadManager.deleteRangeDownload(list.mapToLongArray(DownloadInfo::gid))
    // Delete image files
    if (checked) {
      list.forEach { info ->
        // Delete file
        info.downloadDir?.delete()
        info.tempDownloadDir?.delete()
        // Remove download path
        EhDB.removeDownloadDirname(info.gid)
      }
    }
  }
}

context(_: DialogState)
suspend fun showMoveDownloadLabel(info: GalleryInfo) {
  val defaultLabel = appCtx.getString(R.string.default_download_label_name)
  val labels = buildList {
    add(defaultLabel)
    DownloadManager.labelList.forEach {
      add(it.label)
    }
  }
  val selected = awaitSelectItem(labels, R.string.download_move_dialog_title)
  val downloadInfo = DownloadManager.getDownloadInfo(info.gid) ?: return
  val label = selectedDownloadLabel(labels, selected)
  DownloadManager.changeLabel(listOf(downloadInfo), label)
}

context(_: DialogState)
suspend fun showMoveDownloadLabelList(list: Collection<DownloadInfo>): String? {
  val defaultLabel = appCtx.getString(R.string.default_download_label_name)
  val labels = buildList {
    add(defaultLabel)
    DownloadManager.labelList.forEach {
      add(it.label)
    }
  }
  val selected = awaitSelectItem(labels, R.string.download_move_dialog_title)
  val label = selectedDownloadLabel(labels, selected)
  DownloadManager.changeLabel(list, label)
  return label
}

context(_: DialogState)
suspend fun awaitSelectDate(): String {
  val initial = LocalDate(2007, 3, 21)
  val yesterday = Clock.System.todayIn(TimeZone.UTC).minus(1, DateTimeUnit.DAY)
  val initialMillis = initial.toEpochMillis()
  val yesterdayMillis = yesterday.toEpochMillis()
  val dateRange = initialMillis..yesterdayMillis
  val dateMillis =
    awaitSelectDate(
      title = R.string.go_to,
      yearRange = initial.year..yesterday.year,
      selectableDates =
        object : SelectableDates {
          override fun isSelectableDate(utcTimeMillis: Long): Boolean = utcTimeMillis in dateRange
        },
    )
  return dateMillis.toLocalDateTime().date.toString()
}

context(_: Context, _: DialogState)
suspend fun showRestartDialog() {
  awaitConfirmationOrCancel {
    Text(stringResource(R.string.settings_restart))
  }
  restartApplication()
}
