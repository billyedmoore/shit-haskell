{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
data T (x :: [] Type) (y :: (,) Type Type) = T

main :: IO ()
main = pure ()
