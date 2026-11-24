{-# LANGUAGE NoDataKinds, GADTSyntax #-}

import Data.Kind (Type)

data D :: [Type] -> Type where
  MkD :: D a

main :: IO ()
main = pure ()
