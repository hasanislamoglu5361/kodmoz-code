#import <Cocoa/Cocoa.h>
#import <CoreServices/CoreServices.h>
#include <stdlib.h>

static void (*g_open_callback)(const char *) = NULL;

@interface KodmozOpenHandler : NSObject
- (void)handleOpenDocuments:(NSAppleEventDescriptor *)event
             withReplyEvent:(NSAppleEventDescriptor *)replyEvent;
@end

@implementation KodmozOpenHandler
- (void)handleOpenDocuments:(NSAppleEventDescriptor *)event
             withReplyEvent:(NSAppleEventDescriptor *)replyEvent {
    (void)replyEvent;
    if (g_open_callback == NULL) {
        return;
    }

    NSAppleEventDescriptor *list = [event paramDescriptorForKeyword:keyDirectObject];
    if (list == nil) {
        return;
    }

    for (NSInteger index = 1; index <= [list numberOfItems]; index++) {
        NSAppleEventDescriptor *item = [list descriptorAtIndex:index];
        if (item == nil) {
            continue;
        }

        NSURL *url = [item fileURLValue];
        if (url == nil) {
            NSAppleEventDescriptor *urlDescriptor =
                [item coerceToDescriptorType:typeFileURL];
            NSData *data = [urlDescriptor data];
            if (data != nil) {
                NSString *string = [[NSString alloc] initWithData:data
                                                         encoding:NSUTF8StringEncoding];
                if (string != nil) {
                    url = [NSURL URLWithString:string];
                }
            }
        }

        if (url != nil) {
            g_open_callback(url.path.UTF8String);
        }
    }
}
@end

static void kodmoz_register_handler(void) {
    static KodmozOpenHandler *handler = nil;
    static dispatch_once_t once_token;
    dispatch_once(&once_token, ^{
      handler = [[KodmozOpenHandler alloc] init];
      [[NSAppleEventManager sharedAppleEventManager]
          setEventHandler:handler
              andSelector:@selector(handleOpenDocuments:withReplyEvent:)
            forEventClass:kCoreEventClass
               andEventID:kAEOpenDocuments];
    });
}

void kodmoz_install_open_handler(void (*callback)(const char *)) {
    g_open_callback = callback;

    static id observer = nil;
    static dispatch_once_t once_token;
    dispatch_once(&once_token, ^{
      // NSApplication installs its own default Apple Event handlers while it
      // finishes launching, so our handler has to be registered afterwards.
      observer = [[NSNotificationCenter defaultCenter]
          addObserverForName:NSApplicationWillFinishLaunchingNotification
                      object:nil
                       queue:[NSOperationQueue mainQueue]
                  usingBlock:^(NSNotification *note) {
                    (void)note;
                    kodmoz_register_handler();
                  }];
      (void)observer;
    });
}
