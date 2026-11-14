module CacheBootB where

import {-# SOURCE #-} CacheBootA

scoreFromB :: T -> Int
scoreFromB = score

makeFromB :: Int -> T
makeFromB = make
