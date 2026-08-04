package com.hippo.ehviewer.download.recovery

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.provider.DocumentsContract
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal data class RecoveryScanLimits(
  val maxDepth: Int = 32,
  val maxNodes: Int = 100_000,
  val maxCandidates: Int = 20_000,
  val maxArchiveEntries: Int = 100_000,
  val maxDurationMillis: Long = 2 * 60 * 60 * 1_000L,
)

internal data class RecoveryDocumentNode(
  val rootKey: String,
  val sourceId: String,
  val parentSourceId: String?,
  val uri: Uri,
  val displayName: String,
  val mimeType: String?,
  val size: Long?,
  val modifiedAtMillis: Long?,
  val flags: Int,
  val relativePath: String,
  val depth: Int,
  val isDirectory: Boolean,
)

internal data class RecoveryTreeSnapshot(
  val rootKey: String,
  val nodes: List<RecoveryDocumentNode>,
  val limitExceeded: Boolean,
)

internal object SafTreeWalker {
  private data class PendingDocument(
    val uri: Uri,
    val documentId: String,
    val sourceId: String,
    val relativePath: String,
    val depth: Int,
  )

  suspend fun walk(
    context: Context,
    rootUri: Uri,
    limits: RecoveryScanLimits,
    onDiscovered: suspend (Int) -> Unit = {},
  ): RecoveryTreeSnapshot =
    if (rootUri.scheme == "file" || rootUri.scheme.isNullOrEmpty()) {
      walkPhysical(rootUri, limits, onDiscovered)
    } else {
      walkDocuments(context, rootUri, limits, onDiscovered)
    }

  private suspend fun walkPhysical(
    rootUri: Uri,
    limits: RecoveryScanLimits,
    onDiscovered: suspend (Int) -> Unit,
  ): RecoveryTreeSnapshot {
    val root = File(requireNotNull(rootUri.path) { "Physical recovery root has no path" }).absoluteFile
    require(!Files.isSymbolicLink(root.toPath())) { "Recovery root must not be a symbolic link" }
    require(root.isDirectory) { "Recovery root is not a directory" }
    val rootKey = "file:${root.path}"
    val nodes = ArrayList<RecoveryDocumentNode>()
    val pending = ArrayDeque<Pair<File, RecoveryDocumentNode>>()
    val rootNode =
      RecoveryDocumentNode(
        rootKey = rootKey,
        sourceId = root.path,
        parentSourceId = null,
        uri = Uri.fromFile(root),
        displayName = root.name.ifEmpty { root.path },
        mimeType = DocumentsContract.Document.MIME_TYPE_DIR,
        size = null,
        modifiedAtMillis = root.lastModified().takeIf { it > 0 },
        flags = 0,
        relativePath = "",
        depth = 0,
        isDirectory = true,
      )
    nodes += rootNode
    pending += root to rootNode
    onDiscovered(nodes.size)
    val deadline = SystemClock.elapsedRealtime() + limits.maxDurationMillis
    var limitExceeded = false

    while (pending.isNotEmpty()) {
      currentCoroutineContext().ensureActive()
      if (SystemClock.elapsedRealtime() >= deadline || nodes.size >= limits.maxNodes) {
        limitExceeded = true
        break
      }
      val (directory, parent) = pending.removeFirst()
      val children = directory.listFiles()?.sortedBy { it.name } ?: emptyList()
      for (child in children) {
        currentCoroutineContext().ensureActive()
        if (nodes.size >= limits.maxNodes || SystemClock.elapsedRealtime() >= deadline) {
          limitExceeded = true
          break
        }
        if (Files.isSymbolicLink(child.toPath())) continue
        val isDirectory = child.isDirectory
        val depth = parent.depth + 1
        if (isDirectory && depth > limits.maxDepth) {
          limitExceeded = true
          continue
        }
        val relativePath = parent.relativePath.joinRelative(child.name)
        val node =
          RecoveryDocumentNode(
            rootKey = rootKey,
            sourceId = child.absolutePath,
            parentSourceId = parent.sourceId,
            uri = Uri.fromFile(child),
            displayName = child.name,
            mimeType = if (isDirectory) DocumentsContract.Document.MIME_TYPE_DIR else null,
            size = child.length().takeIf { !isDirectory },
            modifiedAtMillis = child.lastModified().takeIf { it > 0 },
            flags = 0,
            relativePath = relativePath,
            depth = depth,
            isDirectory = isDirectory,
          )
        nodes += node
        onDiscovered(nodes.size)
        if (isDirectory) pending += child to node
      }
    }
    return RecoveryTreeSnapshot(rootKey, nodes, limitExceeded)
  }

  private suspend fun walkDocuments(
    context: Context,
    treeUri: Uri,
    limits: RecoveryScanLimits,
    onDiscovered: suspend (Int) -> Unit,
  ): RecoveryTreeSnapshot {
    val resolver = context.contentResolver
    val rootDocumentId = DocumentsContract.getTreeDocumentId(treeUri)
    val rootDocumentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootDocumentId)
    val authority = requireNotNull(treeUri.authority) { "Recovery tree has no authority" }
    val rootKey = "$authority:$rootDocumentId"
    val rootSourceId = "$authority:$rootDocumentId"
    val nodes = ArrayList<RecoveryDocumentNode>()
    nodes +=
      RecoveryDocumentNode(
        rootKey = rootKey,
        sourceId = rootSourceId,
        parentSourceId = null,
        uri = rootDocumentUri,
        displayName = treeUri.lastPathSegment ?: rootDocumentId,
        mimeType = DocumentsContract.Document.MIME_TYPE_DIR,
        size = null,
        modifiedAtMillis = null,
        flags = 0,
        relativePath = "",
        depth = 0,
        isDirectory = true,
      )
    onDiscovered(nodes.size)

    val pending =
      ArrayDeque<PendingDocument>().apply {
        add(PendingDocument(rootDocumentUri, rootDocumentId, rootSourceId, "", 0))
      }
    val seen = hashSetOf(rootDocumentId)
    val deadline = SystemClock.elapsedRealtime() + limits.maxDurationMillis
    var limitExceeded = false

    while (pending.isNotEmpty()) {
      currentCoroutineContext().ensureActive()
      if (SystemClock.elapsedRealtime() >= deadline || nodes.size >= limits.maxNodes) {
        limitExceeded = true
        break
      }
      val parent = pending.removeFirst()
      val childrenUri =
        DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parent.documentId)
      resolver
        .query(childrenUri, DOCUMENT_PROJECTION, null, null, null)
        ?.use { cursor ->
          val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
          val nameColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
          val mimeColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
          val sizeColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
          val modifiedColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
          val flagsColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_FLAGS)
          while (cursor.moveToNext()) {
            currentCoroutineContext().ensureActive()
            if (nodes.size >= limits.maxNodes || SystemClock.elapsedRealtime() >= deadline) {
              limitExceeded = true
              break
            }
            val documentId = cursor.getString(idColumn)
            if (!seen.add(documentId)) continue
            val displayName =
              nameColumn.takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getString)
                ?: documentId.substringAfterLast('/')
            val mimeType = mimeColumn.takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getString)
            val isDirectory = mimeType == DocumentsContract.Document.MIME_TYPE_DIR
            val depth = parent.depth + 1
            if (isDirectory && depth > limits.maxDepth) {
              limitExceeded = true
              continue
            }
            val sourceId = "$authority:$documentId"
            val relativePath = parent.relativePath.joinRelative(displayName)
            val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
            val node =
              RecoveryDocumentNode(
                rootKey = rootKey,
                sourceId = sourceId,
                parentSourceId = parent.sourceId,
                uri = childUri,
                displayName = displayName,
                mimeType = mimeType,
                size = sizeColumn.longOrNull(cursor),
                modifiedAtMillis = modifiedColumn.longOrNull(cursor),
                flags = flagsColumn.intOrZero(cursor),
                relativePath = relativePath,
                depth = depth,
                isDirectory = isDirectory,
              )
            nodes += node
            onDiscovered(nodes.size)
            if (isDirectory) {
              pending += PendingDocument(childUri, documentId, sourceId, relativePath, depth)
            }
          }
        }
    }
    return RecoveryTreeSnapshot(rootKey, nodes, limitExceeded)
  }

  private fun Int.longOrNull(cursor: android.database.Cursor): Long? =
    takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getLong)

  private fun Int.intOrZero(cursor: android.database.Cursor): Int =
    takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getInt) ?: 0

  private fun String.joinRelative(child: String) = if (isEmpty()) child else "$this/$child"

  private val DOCUMENT_PROJECTION =
    arrayOf(
      DocumentsContract.Document.COLUMN_DOCUMENT_ID,
      DocumentsContract.Document.COLUMN_DISPLAY_NAME,
      DocumentsContract.Document.COLUMN_MIME_TYPE,
      DocumentsContract.Document.COLUMN_SIZE,
      DocumentsContract.Document.COLUMN_LAST_MODIFIED,
      DocumentsContract.Document.COLUMN_FLAGS,
    )
}
