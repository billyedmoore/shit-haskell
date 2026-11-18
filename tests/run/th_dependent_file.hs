{-# LANGUAGE TemplateHaskell #-}
-- `addDependentFile` in a splice (file-embed's `embedFile`, used by hlint) is accepted.
import Language.Haskell.TH.Syntax

main :: IO ()
main = print $(addDependentFile "th_dependent_file.hs" >> lift (3 :: Int))
