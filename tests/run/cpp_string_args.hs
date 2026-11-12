{-# LANGUAGE CPP #-}
-- A string literal's commas and parentheses stay in one macro argument (happy's DEBUG_TRACE).
module Main where

#define TRACE(s) (putStrLn (s)) >>
#define PAIR(a, b) (a, b)

main :: IO ()
main = do
  TRACE("state: " ++ show (1 :: Int) ++
        ",\ttoken: (" ++ show 'x' ++
        ",\taction: ")
    return ()
  print PAIR("a, b", "(c")
  TRACE("\"quoted, still\" one")
    return ()
