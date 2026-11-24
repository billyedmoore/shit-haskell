{-# LANGUAGE UnliftedDatatypes, Haskell2010 #-}

import Data.Kind (Type)

type T :: Type
data T = T deriving Show

main :: IO ()
main = print T
