{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)

data D (a :: [] Type) = D
data T (b :: (,) Type Type) = T

main :: IO ()
main = pure ()
