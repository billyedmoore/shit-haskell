{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
type I a = a
type LocalType = Type
data T (x :: I LocalType) = T

main :: IO ()
main = pure ()
