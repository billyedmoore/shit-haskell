{-# LANGUAGE ScopedTypeVariables #-}
import Control.Exception
import Control.Monad
import Data.IORef
import System.IO
import System.IO.Error

data MyErr = MyErr Int String
  deriving Show

instance Exception MyErr

main :: IO ()
main = do
  r1 <- try (throwIO (MyErr 3 "boom")) :: IO (Either MyErr ())
  print r1
  r2 <- try (evaluate (1 `div` (0 :: Int)))
  case r2 of
    Left DivideByZero -> putStrLn "divide by zero"
    Left e -> putStrLn ("other arith: " ++ show e)
    Right v -> print v
  r3 <- try (evaluate (head ([] :: [Int])))
  case r3 of
    Left (e :: SomeException) -> putStrLn "head failed"
    Right v -> print v
  -- bracket releases in order, also when the body throws.
  logRef <- newIORef []
  let note s = modifyIORef logRef (s :)
  r4 <- try (bracket (note "acquire" >> return 5) (\_ -> note "release") (\x -> note "use" >> when (x > (3 :: Int)) (throwIO (MyErr x "in body")) >> return x))
  print (r4 :: Either MyErr Int)
  readIORef logRef >>= print . reverse
  -- finally and onException.
  (putStrLn "body" `finally` putStrLn "finalizer")
  r5 <- try ((throwIO (ErrorCall "x") `onException` putStrLn "onException ran") :: IO ())
  print (r5 :: Either ErrorCall ())
  -- handleJust and catches.
  r6 <- handleJust (\e -> if isDoesNotExistError e then Just () else Nothing) (\_ -> return "no such file") (readFile "/nonexistent/file" >>= \s -> return (take 3 s))
  putStrLn r6
  let handlers = [Handler (\(e :: ArithException) -> return ("arith " ++ show e)), Handler (\(e :: ErrorCall) -> return ("error " ++ show e))]
  (evaluate (error "E1") `catches` handlers) >>= putStrLn
  (evaluate (2 ^ (-1 :: Int) :: Int) >> return "none") `catches` handlers >>= putStrLn
  -- Nested handlers: the inner one rethrows a different exception.
  r7 <- try (throwIO (MyErr 1 "a") `catch` \(MyErr n s) -> throwIO (MyErr (n + 1) (s ++ "b")))
  print (r7 :: Either MyErr ())
  -- An exception inside a pure value, forced later.
  let xs = [1, 2, error "third"] :: [Int]
  print (length xs)
  r8 <- try (print (sum xs))
  case r8 of
    Left (ErrorCall m) -> putStrLn ("sum failed: " ++ m)
    Right () -> return ()
  -- ioError and userError.
  r9 <- tryIOError (ioError (userError "custom") :: IO ())
  print r9
  -- assert-like: throw in pure code.
  r10 <- try (evaluate (throw (MyErr 9 "pure") :: Int))
  print (r10 :: Either MyErr Int)
  hFlush stdout
  throwIO (MyErr 42 "uncaught")
