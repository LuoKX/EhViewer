package com.hippo.ehviewer.download.recovery

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.provider.DocumentsContract
import com.hippo.ehviewer.bridge.download.RecoveryScanBackend
import com.hippo.ehviewer.bridge.download.RecoveryScanOptions
import com.hippo.ehviewer.bridge.download.RecoveryScanSession
import com.hippo.ehviewer.bridge.files.StorageNameInput
import com.hippo.ehviewer.bridge.files.StorageOperation
import com.hippo.ehviewer.bridge.files.StorageRulesRequest
import com.hippo.ehviewer.bridge.files.planStorageRules
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import java.io.File
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
    val root =
      File(requireNotNull(rootUri.path) { "Physical recovery root has no path" }).absoluteFile
    require(!root.isSymbolicLinkCompat()) { "Recovery root must not be a symbolic link" }
    require(root.isDirectory) { "Recovery root is not a directory" }
    val rootKey = "file:${root.path}"
    val nodes = ArrayList<RecoveryDocumentNode>()
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
    onDiscovered(nodes.size)
    return RecoveryScanSession.open(limits.options(RecoveryScanBackend.PHYSICAL)).use { planner ->
      while (true) {
        currentCoroutineContext().ensureActive()
        val ticket = planner.poll(SystemClock.elapsedRealtime()) ?: break
        val parent = nodes[ticket.index]
        check(parent.isDirectory && parent.depth == ticket.depth) {
          "Invalid recovery parent ticket"
        }
        val children = File(parent.sourceId).listFiles().orEmpty()
        // listFiles remains a platform effect; bound native/name working sets before encoding.
        if (
          children.size > 100_000 ||
            children.sumOf { it.name.length.toLong() * 2 } > 16 * 1024 * 1024
        ) {
          planner.stop()
          planner.finishDirectory()
          break
        }
        val order =
          planStorageRules(
              StorageRulesRequest(
                operation = StorageOperation.NAMES,
                names = children.map { StorageNameInput(it.name.toUtf16LeBytes()) },
              )
            )
            .indices
        for (index in order) {
          currentCoroutineContext().ensureActive()
          if (!planner.gate(SystemClock.elapsedRealtime())) break
          val child = children[index]
          val symbolicLink = child.isSymbolicLinkCompat()
          val isDirectory = !symbolicLink && child.isDirectory
          val childTicket = planner.accept(isDirectory, symbolicLink) ?: continue
          check(childTicket.index == nodes.size) { "Invalid recovery child ticket" }
          val relativePath = parent.relativePath.joinRelative(child.name)
          nodes +=
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
              depth = childTicket.depth,
              isDirectory = isDirectory,
            )
          onDiscovered(nodes.size)
        }
        planner.finishDirectory()
      }
      RecoveryTreeSnapshot(rootKey, nodes, planner.limitExceeded)
    }
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

    return RecoveryScanSession.open(
        limits.options(RecoveryScanBackend.DOCUMENTS, rootDocumentId.toUtf16LeBytes())
      )
      .use { planner ->
        while (true) {
          currentCoroutineContext().ensureActive()
          val ticket = planner.poll(SystemClock.elapsedRealtime()) ?: break
          val parent = nodes[ticket.index]
          check(parent.isDirectory && parent.depth == ticket.depth) {
            "Invalid recovery parent ticket"
          }
          val parentDocumentId = parent.sourceId.substring(authority.length + 1)
          val childrenUri =
            DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocumentId)
          resolver.query(childrenUri, DOCUMENT_PROJECTION, null, null, null)?.use { cursor ->
            val idColumn =
              cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
            val modifiedColumn =
              cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            val flagsColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_FLAGS)
            while (cursor.moveToNext()) {
              currentCoroutineContext().ensureActive()
              if (!planner.gate(SystemClock.elapsedRealtime())) break
              val documentId = cursor.getString(idColumn)
              if (!planner.observeDocument(documentId)) continue
              val displayName =
                nameColumn.takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getString)
                  ?: documentId.substringAfterLast('/')
              val mimeType =
                mimeColumn.takeIf { it >= 0 && !cursor.isNull(it) }?.let(cursor::getString)
              val isDirectory = mimeType == DocumentsContract.Document.MIME_TYPE_DIR
              val childTicket = planner.accept(isDirectory) ?: continue
              check(childTicket.index == nodes.size) { "Invalid recovery child ticket" }
              val sourceId = "$authority:$documentId"
              val relativePath = parent.relativePath.joinRelative(displayName)
              nodes +=
                RecoveryDocumentNode(
                  rootKey = rootKey,
                  sourceId = sourceId,
                  parentSourceId = parent.sourceId,
                  uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId),
                  displayName = displayName,
                  mimeType = mimeType,
                  size = sizeColumn.longOrNull(cursor),
                  modifiedAtMillis = modifiedColumn.longOrNull(cursor),
                  flags = flagsColumn.intOrZero(cursor),
                  relativePath = relativePath,
                  depth = childTicket.depth,
                  isDirectory = isDirectory,
                )
              onDiscovered(nodes.size)
            }
          }
          planner.finishDirectory()
        }
        RecoveryTreeSnapshot(rootKey, nodes, planner.limitExceeded)
      }
  }

  private fun RecoveryScanLimits.options(
    backend: RecoveryScanBackend,
    rootDocumentId: ByteArray = byteArrayOf(),
  ) =
    RecoveryScanOptions(
      backend = backend,
      maxDepth = maxDepth,
      maxNodes = maxNodes,
      startedAtMillis = SystemClock.elapsedRealtime(),
      maxDurationMillis = maxDurationMillis,
      rootDocumentId = rootDocumentId,
    )

  private fun Int.longOrNull(cursor: android.database.Cursor): Long? = takeIf {
    it >= 0 && !cursor.isNull(it)
  }
    ?.let(cursor::getLong)

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

internal fun File.isSymbolicLinkCompat(): Boolean = runCatching {
  val candidate = parentFile?.canonicalFile?.let { File(it, name) } ?: absoluteFile
  candidate.canonicalFile != candidate.absoluteFile
}
  .getOrDefault(true)
