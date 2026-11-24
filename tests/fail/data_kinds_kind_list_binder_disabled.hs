{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)

data D (a :: [Type]) = D

main :: IO ()
main = pure ()
