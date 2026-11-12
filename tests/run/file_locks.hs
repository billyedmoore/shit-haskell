-- The RTS's file locks: many readers or one writer per file; a conflicting open fails with
-- "resource busy (file is locked)" (file-io's concurrency tests).
module Main where

import Control.Exception (IOException, try)
import System.Directory (getTemporaryDirectory, removeFile)
import System.IO

attempt :: IO a -> IO String
attempt act = do
  r <- try act
  pure $ case r of
    Left e -> "error: " ++ dropPath (show (e :: IOException))
    Right _ -> "ok"
  where
    dropPath s = case break (== ':') s of
      (_, ':' : rest) -> dropWhile (== ' ') rest
      _ -> s

main :: IO ()
main = do
  dir <- getTemporaryDirectory
  let f = dir ++ "/hsc-file-locks-test.txt"
  writeFile f "hello"
  withFile f ReadMode $ \_ -> do
    attempt (withFile f ReadMode hGetLine) >>= putStrLn
    attempt (withFile f WriteMode (`hPutStr` "x")) >>= putStrLn
  withFile f AppendMode $ \_ -> do
    attempt (withFile f ReadMode hGetLine) >>= putStrLn
    attempt (withFile f AppendMode (`hPutStr` "y")) >>= putStrLn
  attempt (withFile f WriteMode (`hPutStr` "after")) >>= putStrLn
  readFile f >>= putStrLn
  removeFile f
