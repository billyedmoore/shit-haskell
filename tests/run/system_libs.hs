import System.Directory
import System.Process
import System.Exit
import Data.Time
import Data.Time.Clock.POSIX
import System.Posix.Files (fileExist, getFileStatus, fileSize, isDirectory)
import System.Posix.Process (getProcessID)
import Control.Monad

main :: IO ()
main = do
  let dir = "hsc-test-dir"
  createDirectoryIfMissing True (dir ++ "/sub")
  writeFile (dir ++ "/a.txt") "hello"
  doesFileExist (dir ++ "/a.txt") >>= print
  doesDirectoryExist (dir ++ "/sub") >>= print
  listDirectory dir >>= print . length
  st <- getFileStatus (dir ++ "/a.txt")
  print (fileSize st, isDirectory st)
  fileExist (dir ++ "/missing") >>= print
  renameFile (dir ++ "/a.txt") (dir ++ "/b.txt")
  readFile (dir ++ "/b.txt") >>= putStrLn
  removeDirectoryRecursive dir
  doesDirectoryExist dir >>= print
  out <- readProcess "echo" ["from", "a", "process"] ""
  putStr out
  (code, so, _) <- readProcessWithExitCode "sh" ["-c", "echo x; exit 3"] ""
  print (code, so)
  pid <- getProcessID
  print (pid > 0)
  let day = fromGregorian 2024 2 29
  print (day, addDays 1 day, diffDays (fromGregorian 2025 1 1) day, toGregorian (addGregorianMonthsClip 1 day))
  let t = UTCTime day (secondsToDiffTime 3661)
  print (t, utcTimeToPOSIXSeconds t, formatTime defaultTimeLocale "%Y-%m-%d %H:%M:%S %a %j" t)
  now <- getCurrentTime
  print (now > t)
