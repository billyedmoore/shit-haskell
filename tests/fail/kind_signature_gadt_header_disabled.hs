{-# LANGUAGE GADTs, NoKindSignatures #-}

import Data.Kind (Type)

data T :: Type where
  Mk :: T

main :: IO ()
main = pure ()
