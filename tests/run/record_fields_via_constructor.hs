{-# LANGUAGE DisambiguateRecordFields, NamedFieldPuns, PatternSynonyms #-}
-- Unqualified field names in constructions and patterns of a constructor imported only
-- qualified resolve through the constructor (Cabal's LocalBuildInfo pattern synonym).
module Main where

import qualified Conf as C

pattern Settings :: Int -> String -> Bool -> C.Config
pattern Settings {depthOf, labelOf, loud} =
  C.Config { C.inner = C.Inner { depth = depthOf, label = labelOf }, verbose = loud }

describe :: C.Config -> String
describe C.Config { inner = C.Inner { depth, label }, verbose } = label ++ ":" ++ show depth ++ ":" ++ show verbose

main :: IO ()
main = do
  let c = C.Config { inner = C.Inner { depth = 2, label = "x" }, verbose = True }
  putStrLn (describe c)
  case Settings 5 "s" False of
    Settings { depthOf = d, loud } -> print (d, loud)
  print (C.inner c)
  -- An update through the pattern synonym's fields.
  let s = Settings 1 "one" True
  case s { depthOf = 9, loud = False } of
    Settings d l v -> print (d, l, v)
  print (C.inner (s { labelOf = "renamed" }))
