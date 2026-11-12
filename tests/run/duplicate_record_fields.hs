{-# LANGUAGE DuplicateRecordFields, NamedFieldPuns, RecordWildCards #-}
-- DuplicateRecordFields: two types of a module share a field name; record syntax resolves it
-- through the constructor, derived Show prints the plain name (Cabal's SetupHooks.Internal).
module Main where

import Inputs
import qualified Inputs as I

data Local = A { name :: String, size :: Int } | B { name :: String }
  deriving Show
data Pair = Pair { name :: String, other :: Int } deriving Show

post :: PreConf -> PostConf
post PreConf { buildConfig = b } = PostConf { buildConfig = b ++ "!", extra = ["x"] }

describe :: PostConf -> String
describe PostConf {..} = buildConfig ++ show extra

main :: IO ()
main = do
  let p = mkPre "cfg"
  print p
  print (post p)
  putStrLn (describe (post p))
  print [A { name = "a", size = 1 }, B { name = "b" }]
  print (Pair { name = "p", other = 2 }, onlyHere (Other True))
  let Pair { name } = Pair "q" 3 in putStrLn name
  print (I.PreConf { I.buildConfig = "q", I.verbosity = 2 }, (mkPre "u") { verbosity = 5 })
