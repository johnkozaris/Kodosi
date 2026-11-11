package com.kodosi.mobile.notifications

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.kodosi.mobile.KodosiApplication
import java.time.Duration
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException

class PushWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        if (!FirebaseConfiguration.available) return Result.success()
        return try {
            val model = (applicationContext as KodosiApplication).model
            if (model.synchronizePush()) {
                Result.success()
            } else {
                Result.retry()
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            Result.retry()
        }
    }

    companion object {
        fun enqueue(context: Context) {
            if (!FirebaseConfiguration.available) return
            val work = OneTimeWorkRequestBuilder<PushWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, Duration.ofMinutes(1))
                .build()
            WorkManager.getInstance(context)
                .enqueueUniqueWork("push-registration", ExistingWorkPolicy.KEEP, work)
                .result.get(5, TimeUnit.SECONDS)
        }
    }
}
