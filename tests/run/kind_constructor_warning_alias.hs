{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
type LocalKind = Int
data T (x :: LocalKind) = T

main :: IO ()
main = pure ()
