module Data.Array.MArray (
    MArray(..), newListArray, newArray_, readArray, writeArray, modifyArray, modifyArray',
    getElems, getAssocs, freeze, thaw, unsafeFreeze, unsafeThaw, Ix(..), Array
  ) where

import Data.Array
import Data.Array.Base
