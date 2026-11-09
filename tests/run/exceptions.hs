import Control.Exception
import System.IO

data MyErr = MyErr String deriving Show
instance Exception MyErr

main :: IO ()
main = do
  r <- try (evaluate (1 `div` (0 :: Int))) :: IO (Either ArithException Int)
  print r
  r2 <- try (throwIO (MyErr "bad")) :: IO (Either MyErr ())
  print r2
  handle (\e -> putStrLn ("caught: " ++ show (e :: SomeException))) $ do
    _ <- evaluate (head ([] :: [Int]))
    return ()
  res <- try (readFile "/nonexistent/file") :: IO (Either IOException String)
  putStrLn (either (const "io error") id res)
  (putStrLn "body" >> throwIO (ErrorCall "oops")) `catch` (\(ErrorCall m) -> putStrLn ("error call: " ++ m)) `finally` putStrLn "finally"
