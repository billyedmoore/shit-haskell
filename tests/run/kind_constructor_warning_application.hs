{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
data T (x :: Maybe Int) = T

main :: IO ()
main = pure ()
