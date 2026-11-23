{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
data Mark = MkMark
data T (x :: Mark) = T

main :: IO ()
main = pure ()
