{-# LANGUAGE CPP #-}
module Multi.Internal (feature, describeBase, tag) where

feature :: Int
feature = MULTI_FEATURE

describeBase :: String
#if MIN_VERSION_base(4,18,0)
describeBase = "modern base"
#else
describeBase = "old base"
#endif

tag :: a -> (String, a)
tag = ("tag",)
