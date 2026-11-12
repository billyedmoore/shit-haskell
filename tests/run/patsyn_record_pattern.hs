{-# LANGUAGE PatternSynonyms, RecordWildCards #-}
-- Record-brace patterns on pattern synonyms (`nm@(CExeName{})` in Cabal's SetupHooks.Errors),
-- prefix synonyms with `P{}` and record synonyms with named fields.
module Main where

data C = Lib | NotLib Int String deriving Show

pattern Exe :: Int -> String -> C
pattern Exe n s = NotLib n s

pattern Point :: Int -> Int -> (Int, Int)
pattern Point {px, py} = (px, py)

describe :: C -> String
describe c = case c of
  nm@(Exe{}) -> "exe " ++ show nm
  _ -> "other"

norm :: (Int, Int) -> Int
norm Point {px = x, py = y} = abs x + abs y

swapped :: (Int, Int) -> (Int, Int)
swapped Point {..} = (py, px)

main :: IO ()
main = do
  mapM_ (putStrLn . describe) [Lib, Exe 3 "x"]
  print (norm (3, -4), swapped (1, 2))
